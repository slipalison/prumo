//! Comparador de ativos: junta o que o Prumo já sabe (cotações, fundamentos da
//! CVM, proventos, a série da carteira, os índices) e o que busca na hora para
//! o ativo de fora da carteira (preço histórico e proventos do Yahoo, índices
//! do Banco Central). A regra está em [`crate::domain::compare`]; o método e os
//! limites, em `docs/comparador.md`; o contrato, em `docs/api.md`.
//!
//! Sob demanda quer dizer: **preço e provento**, no máximo uma conferência por
//! símbolo por dia, guardada no mesmo cache das outras telas. Fundamento de
//! ativo de fora da carteira só aparece se já estiver no banco: este módulo
//! nunca dispara a busca da CVM.

use std::collections::{BTreeSet, HashMap};

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::compare::{
    self as rules, Family, Levels, Period, Row, Token, BASE_SLACK_DAYS, MAX_ASSETS,
};
use crate::domain::fundamentals::{AssetFundamentals, Figure, Unsupported};
use crate::domain::performance::months_before;
use crate::domain::retro;
use crate::domain::sector::reference_key;
use crate::domain::tax_book::TaxInstrument;
use crate::history::{HistoryError, HistoryProvider, IndexCode};
use crate::income::{payer_symbol, IncomeService};
use crate::performance::{default_currency, local_date, series as portfolio_series};
use crate::quotes::fx_symbol;
use crate::service::QuoteService;
use crate::store::{Asset, Store, StoreError};

/// Quanto vale uma conferência dos desdobramentos antes de ser refeita.
const SPLIT_CHECK_TTL: Duration = Duration::hours(24);
/// O índice que não tem dado até aqui já foi pedido: não adianta insistir.
const INDEX_FLOORS: [(IndexCode, Date); 2] = [
    (IndexCode::Cdi, rules::MAX_FROM),
    (IndexCode::Ibov, time::macros::date!(1993 - 04 - 27)),
];
/// Folga antes de pedir de novo um índice que parece atrasado ou incompleto.
const INDEX_SLACK_DAYS: i64 = 7;

/// O que a pessoa pediu e não dá para comparar.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CompareError {
    #[error("{0}")]
    Invalid(String),
}

fn invalid(message: impl Into<String>) -> CompareError {
    CompareError::Invalid(message.into())
}

// ------------------------------------------------------------------- ativos --

/// Um ativo da comparação: da carteira ou escolhido na busca.
#[derive(Debug, Clone)]
pub struct Subject {
    /// A forma que a tela usa para casar a coluna com a escolha.
    pub key: String,
    pub symbol: String,
    pub name: String,
    pub class: AssetClass,
    pub listed_on_b3: bool,
    /// O ativo da carteira, se for um.
    pub asset: Option<Asset>,
}

impl Subject {
    pub fn market(&self) -> Market {
        self.class.market(self.listed_on_b3)
    }

    pub fn family(&self) -> Family {
        Family::of(self.class)
    }

    pub fn in_portfolio(&self) -> bool {
        self.asset.is_some()
    }

    /// O símbolo no Yahoo (`PETR4.SA`, `AAPL`, `BTC-USD`).
    pub fn quote_symbol(&self) -> Option<String> {
        self.market().quote_symbol(&self.symbol)
    }

    /// O ativo no formato da carteira, para a leitura dos fundamentos: o da
    /// carteira, ou um de mentira (sem id, sem quantidade) para o de fora.
    fn as_asset(&self) -> Asset {
        self.asset.clone().unwrap_or_else(|| Asset {
            id: Uuid::nil(),
            asset_class: self.class,
            symbol: Some(self.symbol.clone()),
            display_name: self.name.clone(),
            quantity: Decimal::ZERO,
            manual_price: None,
            score_override: None,
            listed_on_b3: self.listed_on_b3,
            treasury: false,
            max_price: None,
            tax_instrument: TaxInstrument::default_for(
                self.class,
                self.listed_on_b3,
                Some(&self.symbol),
                &self.name,
            ),
            tax_instrument_custom: false,
        })
    }
}

/// Lê a lista `ativos` e acha cada item na carteira (pelo símbolo, ou pelo
/// símbolo e a classe quando a classe vem escrita) ou, fora dela, pela classe
/// escrita (ou palpitada). Renda fixa e Tesouro não entram.
pub fn resolve(portfolio: &[Asset], list: &str) -> Result<Vec<Subject>, CompareError> {
    let items: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() || items.len() > MAX_ASSETS {
        return Err(invalid(format!(
            "Escolha de 1 a {MAX_ASSETS} ativos para comparar."
        )));
    }
    let mut subjects: Vec<Subject> = Vec::new();
    for item in items {
        let token: Token =
            rules::parse_token(item).ok_or_else(|| invalid(format!("Ativo invalido: {item}.")))?;
        let subject =
            supported(from_portfolio(portfolio, &token).unwrap_or_else(|| outside(&token)))?;
        let repeated = subjects
            .iter()
            .any(|s| s.symbol == subject.symbol && s.class == subject.class);
        if !repeated {
            subjects.push(subject);
        }
    }
    Ok(subjects)
}

fn from_portfolio(portfolio: &[Asset], token: &Token) -> Option<Subject> {
    let asset = portfolio.iter().find(|a| {
        a.symbol.as_deref() == Some(token.symbol.as_str())
            && token
                .hint
                .is_none_or(|hint| hint == (a.asset_class, a.listed_on_b3))
    })?;
    Some(Subject {
        key: token.key(),
        symbol: token.symbol.clone(),
        name: asset.display_name.clone(),
        class: asset.asset_class,
        listed_on_b3: asset.listed_on_b3,
        asset: Some(asset.clone()),
    })
}

fn outside(token: &Token) -> Subject {
    let (class, listed_on_b3) = token
        .hint
        .unwrap_or_else(|| rules::guess_class(&token.symbol));
    Subject {
        key: token.key(),
        symbol: token.symbol.clone(),
        name: token.symbol.clone(),
        class,
        listed_on_b3,
        asset: None,
    }
}

fn supported(subject: Subject) -> Result<Subject, CompareError> {
    let market = subject.market();
    let treasury = subject.asset.as_ref().is_some_and(|a| a.treasury);
    if treasury || matches!(market, Market::Treasury | Market::Manual) {
        return Err(invalid(format!(
            "{} e renda fixa: o comparador cobre ativos com cotacao em bolsa.",
            subject.symbol
        )));
    }
    Ok(subject)
}

// ---------------------------------------------------------- o que vai buscar --

/// As fontes que o comparador usa.
pub struct Sources<'a> {
    pub store: &'a Store,
    pub history: &'a dyn HistoryProvider,
    pub income: &'a IncomeService,
    pub quotes: &'a QuoteService,
}

/// Garante o preço histórico de `symbol` desde `from`: busca o que o cache não
/// cobre (para trás, ou os desdobramentos conferidos há mais de um dia) e
/// guarda. Falha da fonte não derruba nada: vale o que já está no cache.
async fn ensure_prices(sources: &Sources<'_>, symbol: &str, from: Date, now: OffsetDateTime) {
    if let Err(error) = try_ensure_prices(sources, symbol, from, now).await {
        tracing::warn!(symbol = %symbol, error = %error, "preco do comparador nao guardado");
    }
}

async fn try_ensure_prices(
    sources: &Sources<'_>,
    symbol: &str,
    from: Date,
    now: OffsetDateTime,
) -> Result<(), StoreError> {
    let store = sources.store;
    let covered = store.price_covered_from(symbol).await?;
    let check = store.split_check(symbol).await?;
    let missing_back = covered.is_none_or(|covered| from < covered);
    let stale =
        check.is_none_or(|(checked_from, at)| checked_from > from || now - at > SPLIT_CHECK_TTL);
    if !missing_back && !stale {
        return Ok(());
    }
    let today = local_date(now);
    match sources.history.closes_and_splits(symbol, from, today).await {
        Ok((closes, splits)) => {
            store.insert_prices(symbol, &closes).await?;
            store.set_price_coverage(symbol, from).await?;
            store.save_price_splits(symbol, from, &splits, now).await?;
        }
        Err(HistoryError::NotFound) => {
            // A fonte não conhece o símbolo: não adianta pedir de novo hoje.
            store.set_price_coverage(symbol, from).await?;
            store.save_price_splits(symbol, from, &[], now).await?;
        }
        Err(error) => {
            tracing::warn!(symbol = %symbol, error = %error, "fechamentos do comparador nao baixados");
        }
    }
    Ok(())
}

/// Garante os índices desde `from`: o que falta antes do que está guardado (a
/// carteira pode ser mais nova que a janela pedida) e, se a tarefa diária não
/// os mantém, o que falta depois.
async fn ensure_indexes(sources: &Sources<'_>, codes: &[IndexCode], from: Date, today: Date) {
    for code in codes {
        let Some((_, floor)) = INDEX_FLOORS.iter().find(|(c, _)| c == code) else {
            continue;
        };
        if let Err(error) = try_ensure_index(sources, *code, from.max(*floor), today).await {
            tracing::warn!(index = code.as_str(), error = %error, "indice do comparador nao guardado");
        }
    }
}

async fn try_ensure_index(
    sources: &Sources<'_>,
    code: IndexCode,
    from: Date,
    today: Date,
) -> Result<(), StoreError> {
    let slack = Duration::days(INDEX_SLACK_DAYS);
    let ranges = match sources.store.index_bounds(code).await? {
        None => vec![(from, today)],
        Some((first, last)) => {
            let mut ranges = Vec::new();
            if first > from + slack {
                ranges.push((from, first - Duration::days(1)));
            }
            if last + slack < today {
                ranges.push((last + Duration::days(1), today));
            }
            ranges
        }
    };
    for (start, end) in ranges {
        match sources.history.index(code, start, end).await {
            Ok(values) => sources.store.upsert_index(code, &values).await?,
            Err(error) => {
                tracing::warn!(index = code.as_str(), error = %error, "indice nao baixado");
            }
        }
    }
    Ok(())
}

/// Garante o histórico de proventos dos ativos que pagam (uma conferência por
/// símbolo, no cache das estimativas de renda).
async fn ensure_dividends(sources: &Sources<'_>, subjects: &[Subject], now: OffsetDateTime) {
    let symbols: Vec<String> = subjects
        .iter()
        .filter_map(|s| payer_symbol(s.market(), Some(&s.symbol)))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if !symbols.is_empty() {
        sources.income.fill(&symbols, now).await;
    }
}

/// A moeda do preço do ativo: a da cotação guardada ou a do mercado.
fn currency_of(subject: &Subject, quote_currency: Option<&str>) -> String {
    quote_currency.map_or_else(
        || default_currency(subject.market()).to_owned(),
        str::to_owned,
    )
}

// ---------------------------------------------------------------- a tabela --

/// Uma célula da tabela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub value: Option<Decimal>,
    pub text: Option<String>,
    /// Por que não há valor. Um código estável, que a tela traduz.
    pub reason: Option<&'static str>,
}

impl Cell {
    fn value(value: Decimal) -> Self {
        Self {
            value: Some(value),
            text: None,
            reason: None,
        }
    }

    fn text(text: String) -> Self {
        Self {
            value: None,
            text: Some(text),
            reason: None,
        }
    }

    fn missing(reason: &'static str) -> Self {
        Self {
            value: None,
            text: None,
            reason: Some(reason),
        }
    }

    pub fn applies(&self) -> bool {
        self.reason != Some(NOT_APPLICABLE)
    }
}

pub const NOT_APPLICABLE: &str = "not_applicable";

/// Uma coluna: um ativo e as células dele.
#[derive(Debug, Clone)]
pub struct Column {
    pub subject: Subject,
    pub currency: String,
    pub price: Option<Decimal>,
    pub cells: HashMap<Row, Cell>,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub as_of: Date,
    pub columns: Vec<Column>,
}

/// O que a tela de comparação lê de um ativo: os fundamentos que já estão
/// guardados, as cotações, os proventos e o histórico de 13 meses.
pub async fn table(
    sources: &Sources<'_>,
    subjects: &[Subject],
    now: OffsetDateTime,
) -> Result<Table, StoreError> {
    let today = local_date(now);
    let from = months_before(today, 13);
    let store = sources.store;

    let symbols: Vec<String> = subjects.iter().filter_map(Subject::quote_symbol).collect();
    futures_util::future::join_all(
        symbols
            .iter()
            .map(|symbol| ensure_prices(sources, symbol, from, now)),
    )
    .await;
    ensure_dividends(sources, subjects, now).await;

    let mut stored = store.quotes(&symbols).await?;
    // O ativo de fora da carteira ainda não tem cotação guardada: pergunta ao
    // provedor (só para quem falta; nada é gravado).
    let missing: Vec<&String> = symbols
        .iter()
        .filter(|symbol| !stored.contains_key(*symbol))
        .collect();
    let fetched = futures_util::future::join_all(
        missing
            .iter()
            .map(|symbol| async move { (*symbol, sources.quotes.provider().quote(symbol).await) }),
    )
    .await;
    let mut live: HashMap<String, (Decimal, String, Option<String>)> = HashMap::new();
    for (symbol, quote) in fetched {
        if let Ok(quote) = quote {
            live.insert(symbol.clone(), (quote.price, quote.currency, quote.name));
        }
    }
    stored.retain(|_, quote| quote.price > Decimal::ZERO);

    let closes = store.price_series(&symbols, from, today).await?;
    let splits = store.price_splits(&symbols).await?;
    let dividend_symbols: Vec<String> = subjects
        .iter()
        .filter_map(|s| payer_symbol(s.market(), Some(&s.symbol)))
        .collect();
    let dividends = store.dividend_cache(&dividend_symbols).await?;
    let assets: Vec<Asset> = subjects.iter().map(Subject::as_asset).collect();
    let asset_refs: Vec<&Asset> = assets.iter().collect();
    let fundamentals = store.fundamentals_for_assets(&asset_refs).await?;
    let sector_keys: Vec<_> = subjects
        .iter()
        .filter_map(|s| reference_key(s.class, &s.symbol))
        .collect();
    let sectors = store.sector_reference(&sector_keys).await?;

    let mut columns = Vec::with_capacity(subjects.len());
    for subject in subjects {
        let quote_symbol = subject.quote_symbol();
        let quote = quote_symbol.as_ref().and_then(|s| stored.get(s));
        let live_quote = quote_symbol.as_ref().and_then(|s| live.get(s));
        let adjusted: Levels = quote_symbol
            .as_ref()
            .and_then(|s| closes.get(s).map(|c| (s, c)))
            .map(|(s, c)| rules::adjust_for_splits(c, splits.get(s).map_or(&[][..], Vec::as_slice)))
            .unwrap_or_default();
        let currency = currency_of(
            subject,
            quote
                .map(|q| q.currency.as_str())
                .or(live_quote.map(|(_, currency, _)| currency.as_str())),
        );
        let price = quote
            .map(|q| q.price)
            .or(live_quote.map(|(price, _, _)| *price))
            .or_else(|| adjusted.last().map(|(_, close)| *close));
        let mut subject = subject.clone();
        if subject.asset.is_none() {
            if let Some(name) = quote
                .and_then(|q| q.name.clone())
                .or_else(|| live_quote.and_then(|(_, _, name)| name.clone()))
            {
                subject.name = name;
            }
        }
        let events: Vec<(Date, Decimal)> = payer_symbol(subject.market(), Some(&subject.symbol))
            .and_then(|s| dividends.get(&s))
            .map(|cached| {
                cached
                    .events
                    .iter()
                    .map(|e| (e.ex_date, e.amount))
                    .collect()
            })
            .unwrap_or_default();
        let has_dividend_history = payer_symbol(subject.market(), Some(&subject.symbol))
            .is_some_and(|s| dividends.contains_key(&s));
        let segment = reference_key(subject.class, &subject.symbol)
            .and_then(|key| sectors.get(&key))
            .map(|info| info.sector.clone());
        let inputs = Inputs {
            price,
            adjusted: &adjusted,
            events: &events,
            has_dividend_history,
            fundamentals: fundamentals.get(&subject.symbol),
            segment,
            today,
        };
        let cells = Row::ALL
            .iter()
            .map(|row| (*row, cell(&subject, *row, &inputs)))
            .collect();
        columns.push(Column {
            subject,
            currency,
            price,
            cells,
        });
    }
    Ok(Table {
        as_of: today,
        columns,
    })
}

struct Inputs<'a> {
    price: Option<Decimal>,
    adjusted: &'a [(Date, Decimal)],
    events: &'a [(Date, Decimal)],
    has_dividend_history: bool,
    fundamentals: Option<&'a AssetFundamentals>,
    segment: Option<String>,
    today: Date,
}

/// A razão de um indicador de fundamentos faltar, quando o ativo nem tem
/// fundamentos: o motivo da CVM, ou "fora da carteira" se a busca ainda não
/// rodou para ele (o comparador não a dispara).
fn without_fundamentals(subject: &Subject, inputs: &Inputs<'_>) -> &'static str {
    match inputs.fundamentals {
        Some(AssetFundamentals::Unsupported(Unsupported::Pending)) | None => {
            if subject.in_portfolio() {
                "pending"
            } else {
                "not_tracked"
            }
        }
        Some(AssetFundamentals::Unsupported(reason)) => reason.as_str(),
        Some(_) => NOT_APPLICABLE,
    }
}

fn figure(figure: Figure) -> Cell {
    figure.value().map_or_else(
        || Cell::missing(figure.gap().map_or("not_reported", |g| g.as_str())),
        Cell::value,
    )
}

fn percent_figure(value: Figure) -> Cell {
    figure(value).map_value(rules::percent)
}

impl Cell {
    fn map_value(mut self, f: impl FnOnce(Decimal) -> Decimal) -> Self {
        self.value = self.value.map(f);
        self
    }
}

fn ratio(numerator: Option<Decimal>, denominator: &Figure) -> Cell {
    match (numerator, denominator.value()) {
        (None, _) => Cell::missing("no_price"),
        (_, None) => Cell::missing(denominator.gap().map_or("not_reported", |g| g.as_str())),
        (Some(_), Some(base)) if base <= Decimal::ZERO => Cell::missing("non_positive_base"),
        (Some(price), Some(base)) => Cell::value((price / base).round_dp(4)),
    }
}

fn cell(subject: &Subject, row: Row, inputs: &Inputs<'_>) -> Cell {
    let family = subject.family();
    if !row.applies_to(family) {
        return Cell::missing(NOT_APPLICABLE);
    }
    let company = match inputs.fundamentals {
        Some(AssetFundamentals::Company(metrics)) => Some(metrics),
        _ => None,
    };
    let fund = match inputs.fundamentals {
        Some(AssetFundamentals::Fund(metrics)) => Some(metrics),
        _ => None,
    };
    // Linha de fundamentos de companhia: sem o balanço, o motivo.
    let company_row = |build: &dyn Fn(&crate::domain::fundamentals::CompanyMetrics) -> Cell| {
        company.map_or_else(
            || Cell::missing(without_fundamentals(subject, inputs)),
            |c| build(c),
        )
    };
    let period_row = |pick: &dyn Fn(&crate::domain::fundamentals::PeriodFigures) -> Figure,
                      percent: bool| {
        company_row(&|c| {
            c.trailing.as_ref().map_or_else(
                || Cell::missing("no_filing"),
                |t| {
                    if percent {
                        percent_figure(pick(t))
                    } else {
                        figure(pick(t))
                    }
                },
            )
        })
    };
    let fund_row = |build: &dyn Fn(&crate::domain::fundamentals::FundMetrics) -> Cell| {
        fund.map_or_else(
            || Cell::missing(without_fundamentals(subject, inputs)),
            |f| build(f),
        )
    };
    match row {
        Row::Price => inputs
            .price
            .map_or_else(|| Cell::missing("no_price"), Cell::value),
        Row::Return12m => {
            if inputs.adjusted.is_empty() {
                Cell::missing("no_price_history")
            } else {
                rules::trailing_return(inputs.adjusted, inputs.today).map_or_else(
                    || Cell::missing("short_history"),
                    |r| Cell::value(rules::percent(r)),
                )
            }
        }
        Row::DividendYield12m => match (inputs.has_dividend_history, inputs.price) {
            (false, _) => Cell::missing("no_dividend_history"),
            (true, None) => Cell::missing("no_price"),
            (true, Some(price)) => rules::dividend_yield(inputs.events, price, inputs.today)
                .map_or_else(
                    || Cell::missing("no_price"),
                    |y| Cell::value(rules::percent(y)),
                ),
        },
        Row::PriceToBook => match (company, fund) {
            (Some(c), _) => ratio(inputs.price, &c.book_value_per_share),
            (_, Some(f)) => match (inputs.price, f.nav_per_share.value()) {
                (Some(price), Some(nav)) if nav > Decimal::ZERO => {
                    Cell::value((price / nav).round_dp(4))
                }
                _ => figure(f.price_to_book),
            },
            _ => Cell::missing(without_fundamentals(subject, inputs)),
        },
        Row::Segment => match (&inputs.segment, subject.class) {
            (Some(text), _) => Cell::text(text.clone()),
            (None, AssetClass::BrStocks | AssetClass::BrReits) => Cell::missing("no_sector"),
            (None, _) => Cell::missing("no_sector_source"),
        },
        Row::PriceToEarnings => company_row(&|c| ratio(inputs.price, &c.earnings_per_share)),
        Row::Roe => period_row(&|t| t.roe, true),
        Row::NetMargin => period_row(&|t| t.net_margin, true),
        Row::EbitdaMargin => period_row(&|t| t.ebitda_margin, true),
        Row::NetDebtToEbitda => period_row(&|t| t.net_debt_to_ebitda, false),
        Row::NetDebtToEquity => period_row(&|t| t.net_debt_to_equity, false),
        Row::RevenueCagr => company_row(&|c| percent_figure(c.revenue_cagr)),
        Row::EarningsCagr => company_row(&|c| percent_figure(c.earnings_cagr)),
        Row::EarningsPerShare => company_row(&|c| figure(c.earnings_per_share)),
        Row::BookValuePerShare => company_row(&|c| figure(c.book_value_per_share)),
        Row::MarketCap => company_row(&|c| match (c.shares, inputs.price) {
            (None, _) => Cell::missing("no_shares"),
            (_, None) => Cell::missing("no_price"),
            (Some(shares), Some(price)) => Cell::value((shares * price).round_dp(2)),
        }),
        Row::NavPerShare => fund_row(&|f| figure(f.nav_per_share)),
        Row::Vacancy => fund_row(&|f| percent_figure(f.vacancy)),
        Row::NetAssets => fund_row(&|f| figure(f.net_assets)),
    }
}

// ------------------------------------------------------------- as curvas --

/// O que a pessoa escolheu na tela de rentabilidade.
#[derive(Debug, Clone)]
pub struct CurveRequest {
    pub period: Period,
    /// Com proventos (retorno total): o provento bruto é reinvestido na data ex.
    pub total_return: bool,
    /// Igualar o início: todas as curvas partem da data em que todas existem.
    pub align_start: bool,
    /// Põe a própria carteira entre as curvas.
    pub portfolio: bool,
    pub benchmarks: Vec<IndexCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    Asset,
    Portfolio,
    Index,
}

#[derive(Debug, Clone)]
pub struct Curve {
    pub key: String,
    pub kind: CurveKind,
    pub name: String,
    /// Em %, uma por data do eixo; `None` antes de a série existir.
    pub values: Vec<Option<f64>>,
    pub return_percent: Option<f64>,
    /// Onde a curva abre em 0%.
    pub starts_on: Option<Date>,
    /// Por que não há curva.
    pub reason: Option<&'static str>,
    /// Os proventos entram na conta desta curva.
    pub includes_income: bool,
    /// O preço foi levado a reais pelo câmbio do dia.
    pub converted: bool,
}

#[derive(Debug, Clone)]
pub struct Curves {
    pub period: Period,
    pub from: Date,
    pub to: Date,
    pub dates: Vec<Date>,
    pub curves: Vec<Curve>,
}

struct Input {
    key: String,
    kind: CurveKind,
    name: String,
    levels: Levels,
    reason: Option<&'static str>,
    includes_income: bool,
    converted: bool,
}

fn float(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

/// As curvas de rentabilidade normalizadas em 0% no começo.
pub async fn curves(
    sources: &Sources<'_>,
    user: Uuid,
    subjects: &[Subject],
    request: &CurveRequest,
    now: OffsetDateTime,
) -> Result<Curves, StoreError> {
    let today = local_date(now);
    let store = sources.store;
    let period_start = request.period.start(today);
    let fetch_from = period_start - Duration::days(BASE_SLACK_DAYS);

    // Moeda de cada ativo (a cotação guardada manda; sem ela, a do mercado) e
    // o câmbio das que não são reais.
    let symbols: Vec<String> = subjects.iter().filter_map(Subject::quote_symbol).collect();
    let quotes = store.quotes(&symbols).await?;
    let currency: HashMap<String, String> = subjects
        .iter()
        .filter_map(|s| {
            let symbol = s.quote_symbol()?;
            let own = quotes.get(&symbol).map(|q| q.currency.as_str());
            Some((symbol, currency_of(s, own)))
        })
        .collect();
    let fx: Vec<String> = currency
        .values()
        .filter(|c| c.as_str() != "BRL")
        .map(|c| fx_symbol(c))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let wanted: Vec<String> = symbols.iter().chain(&fx).cloned().collect();

    futures_util::future::join_all(
        wanted
            .iter()
            .map(|symbol| ensure_prices(sources, symbol, fetch_from, now)),
    )
    .await;
    if request.total_return {
        ensure_dividends(sources, subjects, now).await;
    }
    ensure_indexes(sources, &request.benchmarks, fetch_from, today).await;

    let closes = store.price_series(&wanted, fetch_from, today).await?;
    let splits = store.price_splits(&symbols).await?;
    let dividend_symbols: Vec<String> = subjects
        .iter()
        .filter_map(|s| payer_symbol(s.market(), Some(&s.symbol)))
        .collect();
    let dividends = store.dividend_cache(&dividend_symbols).await?;

    let mut inputs: Vec<Input> = Vec::new();
    for subject in subjects {
        let symbol = subject.quote_symbol();
        let price: Levels = symbol
            .as_ref()
            .and_then(|s| closes.get(s).map(|c| (s, c)))
            .map(|(s, c)| rules::adjust_for_splits(c, splits.get(s).map_or(&[][..], Vec::as_slice)))
            .unwrap_or_default();
        let events: Vec<(Date, Decimal)> = payer_symbol(subject.market(), Some(&subject.symbol))
            .and_then(|s| dividends.get(&s))
            .map(|cached| {
                cached
                    .events
                    .iter()
                    .map(|e| (e.ex_date, e.amount))
                    .collect()
            })
            .unwrap_or_default();
        let counts = request.total_return && !events.is_empty();
        let levels = if counts {
            rules::total_return_levels(&price, &events)
        } else {
            price
        };
        let own = symbol.as_ref().and_then(|s| currency.get(s));
        let (levels, converted) = match own.filter(|c| c.as_str() != "BRL") {
            Some(own) => {
                let rate = closes.get(&fx_symbol(own));
                (retro::in_reais(&levels, rate), true)
            }
            None => (levels, false),
        };
        let reason = levels.is_empty().then_some("no_price_history");
        inputs.push(Input {
            key: subject.key.clone(),
            kind: CurveKind::Asset,
            name: subject.name.clone(),
            levels,
            reason,
            includes_income: counts,
            converted,
        });
    }
    if request.portfolio {
        let points = portfolio_series(store, user, now).await?;
        let levels: Levels = points.iter().map(|p| (p.date, p.quota_value)).collect();
        let reason = levels.is_empty().then_some("no_series");
        inputs.push(Input {
            key: "portfolio".to_owned(),
            kind: CurveKind::Portfolio,
            name: "Carteira".to_owned(),
            levels,
            reason,
            includes_income: true,
            converted: false,
        });
    }
    for code in &request.benchmarks {
        let rows = store.index_series(*code, fetch_from, today).await?;
        // Índice de nível (pontos) já é a curva; o de taxa vira nível. Sem
        // `_`: índice novo precisa dizer qual dos dois é.
        let levels = match code {
            IndexCode::Ibov | IndexCode::Ifix => rows,
            IndexCode::Cdi | IndexCode::Selic | IndexCode::Ipca => rules::rate_levels(&rows),
        };
        let reason = levels.is_empty().then_some("no_index_data");
        inputs.push(Input {
            key: code.as_str().to_owned(),
            kind: CurveKind::Index,
            name: code.label().to_owned(),
            levels,
            reason,
            includes_income: true,
            converted: false,
        });
    }

    // Onde a comparação começa.
    let first_of = |input: &Input| input.levels.first().map(|(d, _)| *d);
    let base_from = if request.period == Period::Max {
        inputs
            .iter()
            .filter(|i| i.kind != CurveKind::Index)
            .filter_map(first_of)
            .min()
            .unwrap_or(today)
    } else {
        period_start
    };
    let from = if request.align_start {
        let starts: Vec<Date> = inputs.iter().filter_map(first_of).collect();
        rules::aligned_start(base_from, &starts)
    } else {
        base_from
    }
    .min(today);

    let all: Vec<&[(Date, Decimal)]> = inputs.iter().map(|i| i.levels.as_slice()).collect();
    let dates = rules::axis(&all, from, today);
    let curves = inputs
        .into_iter()
        .map(|input| {
            let normalized = rules::normalize(&input.levels, from, &dates);
            let return_percent = rules::last_value(&normalized.values)
                .filter(|_| normalized.starts_on.is_some())
                .map(|r| float(rules::percent(r)));
            Curve {
                key: input.key,
                kind: input.kind,
                name: input.name,
                values: normalized
                    .values
                    .iter()
                    .map(|v| v.map(|v| float(rules::percent(v))))
                    .collect(),
                return_percent,
                starts_on: normalized.starts_on,
                reason: input.reason,
                includes_income: input.includes_income,
                converted: input.converted,
            }
        })
        .collect();
    Ok(Curves {
        period: request.period,
        from,
        to: today,
        dates,
        curves,
    })
}
