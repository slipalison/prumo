//! Comparador de ativos: a regra pura. Quem escolhe o que comparar (o
//! símbolo na URL), os períodos, as curvas normalizadas em 0% no começo e o
//! destaque neutro do maior ou do menor valor de cada linha. Sem banco, sem
//! rede. O método, as fontes e os limites estão em `docs/comparador.md`.
//!
//! O destaque **não é ranking nem recomendação**: marca só o extremo da linha
//! numa direção fixa por indicador (a tela diz qual na dica), e só entre ativos
//! do mesmo tipo (empresa com empresa, fundo com fundo).

use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, Duration};

use super::asset_class::{normalize_symbol, AssetClass};
use super::performance::months_before;
use super::retro::Series;

/// Quantos ativos cabem numa comparação.
pub const MAX_ASSETS: usize = 5;
/// Pontos que a curva pode ter: acima disso o eixo é afinado (a tela não ganha
/// nada com mais de um ponto por pixel).
pub const MAX_POINTS: usize = 1_200;
/// Dias de folga antes do começo do período, para haver um fechamento de base
/// mesmo quando o começo cai em fim de semana ou feriado.
pub const BASE_SLACK_DAYS: i64 = 10;
/// De onde o período "máximo" parte: antes disso nenhuma fonte tem dado.
pub const MAX_FROM: Date = time::macros::date!(1990 - 01 - 01);

/// Um valor numa data: o nível de uma série (preço, índice, cota).
pub type Levels = Series;

// ----------------------------------------------------------------- períodos --

/// Os períodos da tela, do mês ao máximo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    OneMonth,
    ThreeMonths,
    SixMonths,
    YearToDate,
    TwelveMonths,
    ThreeYears,
    FiveYears,
    Max,
}

impl Period {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "1m" => Some(Period::OneMonth),
            "3m" => Some(Period::ThreeMonths),
            "6m" => Some(Period::SixMonths),
            "ytd" => Some(Period::YearToDate),
            "12m" => Some(Period::TwelveMonths),
            "3y" => Some(Period::ThreeYears),
            "5y" => Some(Period::FiveYears),
            "max" => Some(Period::Max),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Period::OneMonth => "1m",
            Period::ThreeMonths => "3m",
            Period::SixMonths => "6m",
            Period::YearToDate => "ytd",
            Period::TwelveMonths => "12m",
            Period::ThreeYears => "3y",
            Period::FiveYears => "5y",
            Period::Max => "max",
        }
    }

    /// A data de base do período que termina em `today` (o fechamento de onde
    /// se mede). No máximo, a data mais antiga que alguma fonte pode ter.
    pub fn start(self, today: Date) -> Date {
        match self {
            Period::OneMonth => months_before(today, 1),
            Period::ThreeMonths => months_before(today, 3),
            Period::SixMonths => months_before(today, 6),
            Period::YearToDate => {
                Date::from_calendar_date(today.year() - 1, time::Month::December, 31)
                    .unwrap_or(today)
            }
            Period::TwelveMonths => months_before(today, 12),
            Period::ThreeYears => months_before(today, 36),
            Period::FiveYears => months_before(today, 60),
            Period::Max => MAX_FROM,
        }
    }
}

// ------------------------------------------------------------------ ativos --

/// O que a pessoa pediu na URL, sem saber ainda se está na carteira:
/// `PETR4`, `KNRI11:br_reits` ou `VALE:intl_stocks:b3` (BDR/ETF da B3 numa
/// classe de fora).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub symbol: String,
    pub hint: Option<(AssetClass, bool)>,
}

impl Token {
    /// A forma canônica, a mesma que a tela usa para casar a coluna com a
    /// escolha.
    pub fn key(&self) -> String {
        match self.hint {
            None => self.symbol.clone(),
            Some((class, true)) if class.accepts_b3_listing() => {
                format!("{}:{}:b3", self.symbol, class.as_str())
            }
            Some((class, _)) => format!("{}:{}", self.symbol, class.as_str()),
        }
    }
}

/// Lê um item da lista `ativos`. `None` se não parece um símbolo ou se a
/// classe é desconhecida.
pub fn parse_token(raw: &str) -> Option<Token> {
    let mut parts = raw.split(':');
    let symbol = normalize_symbol(parts.next()?)?;
    let hint = match parts.next().map(str::trim).filter(|p| !p.is_empty()) {
        None => None,
        Some(class) => {
            let class = AssetClass::parse(class)?;
            let b3 = match parts.next().map(str::trim) {
                None | Some("") => class.native_market() == super::asset_class::Market::B3,
                Some("b3") => true,
                Some(_) => return None,
            };
            Some((class, b3))
        }
    };
    parts.next().is_none().then_some(Token { symbol, hint })
}

/// A classe palpitada para um símbolo que a pessoa digitou na mão, fora da
/// carteira e sem a classe: ticker da B3 (quatro letras e um ou dois dígitos)
/// terminado em 11 é fundo; sem o 11, ação; o resto é ação dos EUA. É só o
/// palpite do endereço digitado: a busca da tela sempre manda a classe.
pub fn guess_class(symbol: &str) -> (AssetClass, bool) {
    let bytes = symbol.as_bytes();
    let b3 = (5..=6).contains(&bytes.len())
        && bytes[..4].iter().all(u8::is_ascii_uppercase)
        && bytes[4..].iter().all(u8::is_ascii_digit);
    match (b3, symbol.ends_with("11")) {
        (true, true) => (AssetClass::BrReits, true),
        (true, false) => (AssetClass::BrStocks, true),
        _ => (AssetClass::IntlStocks, false),
    }
}

/// A que família de indicadores o ativo pertence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Company,
    Fund,
    Other,
}

impl Family {
    pub fn of(class: AssetClass) -> Self {
        match class {
            AssetClass::BrStocks | AssetClass::IntlStocks => Family::Company,
            AssetClass::BrReits | AssetClass::UsReits => Family::Fund,
            AssetClass::Crypto | AssetClass::FixedIncome | AssetClass::IntlFixedIncome => {
                Family::Other
            }
        }
    }
}

// ------------------------------------------------------------- indicadores --

/// De que lado fica o destaque da linha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Marca o maior valor.
    Higher,
    /// Marca o menor valor.
    Lower,
    /// Sem destaque: o número não tem lado (preço, patrimônio).
    None,
}

/// Como o número é escrito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// `12,3` (múltiplo, sem sufixo; a tela põe o "×").
    Multiple,
    /// Já em percentual (`12,3` = 12,3%).
    Percent,
    /// Dinheiro na moeda do ativo, com centavos.
    Money,
    /// Dinheiro em reais, compacto (R$ 3,1 bi).
    MoneyCompact,
    /// Texto (setor, tipo de fundo).
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    General,
    Company,
    Fund,
}

/// Cada linha da tabela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Row {
    Price,
    #[serde(rename = "return_12m")]
    Return12m,
    #[serde(rename = "dividend_yield_12m")]
    DividendYield12m,
    PriceToBook,
    Segment,
    PriceToEarnings,
    Roe,
    NetMargin,
    EbitdaMargin,
    NetDebtToEbitda,
    NetDebtToEquity,
    RevenueCagr,
    EarningsCagr,
    EarningsPerShare,
    BookValuePerShare,
    MarketCap,
    NavPerShare,
    Vacancy,
    NetAssets,
}

impl Row {
    /// Na ordem em que a tabela mostra.
    pub const ALL: [Row; 19] = [
        Row::Price,
        Row::Return12m,
        Row::DividendYield12m,
        Row::PriceToBook,
        Row::Segment,
        Row::PriceToEarnings,
        Row::Roe,
        Row::NetMargin,
        Row::EbitdaMargin,
        Row::NetDebtToEbitda,
        Row::NetDebtToEquity,
        Row::RevenueCagr,
        Row::EarningsCagr,
        Row::EarningsPerShare,
        Row::BookValuePerShare,
        Row::MarketCap,
        Row::NavPerShare,
        Row::Vacancy,
        Row::NetAssets,
    ];

    pub fn direction(self) -> Direction {
        match self {
            Row::PriceToBook
            | Row::PriceToEarnings
            | Row::NetDebtToEbitda
            | Row::NetDebtToEquity
            | Row::Vacancy => Direction::Lower,
            Row::Return12m
            | Row::DividendYield12m
            | Row::Roe
            | Row::NetMargin
            | Row::EbitdaMargin
            | Row::RevenueCagr
            | Row::EarningsCagr => Direction::Higher,
            Row::Price
            | Row::Segment
            | Row::EarningsPerShare
            | Row::BookValuePerShare
            | Row::MarketCap
            | Row::NavPerShare
            | Row::NetAssets => Direction::None,
        }
    }

    pub fn unit(self) -> Unit {
        match self {
            Row::PriceToBook
            | Row::PriceToEarnings
            | Row::NetDebtToEbitda
            | Row::NetDebtToEquity => Unit::Multiple,
            Row::Return12m
            | Row::DividendYield12m
            | Row::Roe
            | Row::NetMargin
            | Row::EbitdaMargin
            | Row::RevenueCagr
            | Row::EarningsCagr
            | Row::Vacancy => Unit::Percent,
            Row::Price | Row::EarningsPerShare | Row::BookValuePerShare | Row::NavPerShare => {
                Unit::Money
            }
            Row::MarketCap | Row::NetAssets => Unit::MoneyCompact,
            Row::Segment => Unit::Text,
        }
    }

    pub fn group(self) -> Group {
        match self {
            Row::Price
            | Row::Return12m
            | Row::DividendYield12m
            | Row::PriceToBook
            | Row::Segment => Group::General,
            Row::PriceToEarnings
            | Row::Roe
            | Row::NetMargin
            | Row::EbitdaMargin
            | Row::NetDebtToEbitda
            | Row::NetDebtToEquity
            | Row::RevenueCagr
            | Row::EarningsCagr
            | Row::EarningsPerShare
            | Row::BookValuePerShare
            | Row::MarketCap => Group::Company,
            Row::NavPerShare | Row::Vacancy | Row::NetAssets => Group::Fund,
        }
    }

    /// Se a linha existe para a família: fora disso a célula diz "não se
    /// aplica" e a linha some se nenhum ativo a tem.
    pub fn applies_to(self, family: Family) -> bool {
        match self {
            Row::Price | Row::Return12m | Row::Segment => true,
            Row::DividendYield12m | Row::PriceToBook => family != Family::Other,
            _ => match self.group() {
                Group::Company => family == Family::Company,
                Group::Fund => family == Family::Fund,
                Group::General => true,
            },
        }
    }
}

/// O extremo marcado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    Max,
    Min,
}

/// O destaque de uma linha. `cells` traz a família e o valor de cada ativo, na
/// ordem das colunas. Só entre ativos da mesma família, e só com dois ou mais
/// valores conhecidos (um valor sozinho não é extremo de nada); empate marca
/// todos. `Direction::None` não marca nada.
pub fn marks(cells: &[(Family, Option<Decimal>)], direction: Direction) -> Vec<Option<Mark>> {
    let mut out = vec![None; cells.len()];
    let mark = match direction {
        Direction::Higher => Mark::Max,
        Direction::Lower => Mark::Min,
        Direction::None => return out,
    };
    for family in [Family::Company, Family::Fund, Family::Other] {
        let known: Vec<(usize, Decimal)> = cells
            .iter()
            .enumerate()
            .filter(|(_, (f, _))| *f == family)
            .filter_map(|(i, (_, v))| v.map(|v| (i, v)))
            .collect();
        if known.len() < 2 {
            continue;
        }
        let extreme = known.iter().map(|(_, v)| *v).reduce(|a, b| match mark {
            Mark::Max => a.max(b),
            Mark::Min => a.min(b),
        });
        for (i, v) in known {
            if Some(v) == extreme {
                out[i] = Some(mark);
            }
        }
    }
    out
}

// ------------------------------------------------------------------ séries --

/// O último nível em ou antes de `date`.
pub fn value_at(levels: &[(Date, Decimal)], date: Date) -> Option<Decimal> {
    let after = levels.partition_point(|(d, _)| *d <= date);
    after.checked_sub(1).map(|i| levels[i].1)
}

/// Desfaz o desdobramento no preço de antes dele: o fechamento guardado é o
/// real do dia, e a comparação precisa do preço na base de hoje. O ajustado é
/// o real dividido pelo produto dos fatores dos eventos **depois** da data.
pub fn adjust_for_splits(closes: &[(Date, Decimal)], splits: &[(Date, Decimal)]) -> Levels {
    closes
        .iter()
        .map(|(date, close)| {
            let factor: Decimal = splits
                .iter()
                .filter(|(when, factor)| when > date && *factor > Decimal::ZERO)
                .map(|(_, factor)| *factor)
                .product();
            let adjusted = if factor > Decimal::ZERO {
                close / factor
            } else {
                *close
            };
            (*date, adjusted.round_dp(8))
        })
        .collect()
}

/// O índice de retorno total: o provento bruto entra no dia da data ex e é
/// reinvestido no preço dele. `closes` já ajustado por desdobramento;
/// `dividends` é por cota, na mesma moeda e base. O nível do primeiro dia é o
/// preço dele; cada dia seguinte multiplica por `(preço + provento) / preço
/// do dia anterior`. Provento com data ex fora do intervalo de fechamentos
/// (antes do primeiro, depois do último) fica de fora.
pub fn total_return_levels(closes: &[(Date, Decimal)], dividends: &[(Date, Decimal)]) -> Levels {
    let mut out: Levels = Vec::with_capacity(closes.len());
    let mut previous: Option<(Date, Decimal)> = None;
    let mut level = Decimal::ZERO;
    for (date, close) in closes {
        match previous {
            None => level = *close,
            Some((before, price)) if price > Decimal::ZERO => {
                let paid: Decimal = dividends
                    .iter()
                    .filter(|(ex, _)| *ex > before && *ex <= *date)
                    .map(|(_, amount)| *amount)
                    .sum();
                level = level * (*close + paid) / price;
            }
            Some(_) => level = *close,
        }
        out.push((*date, level));
        previous = Some((*date, *close));
    }
    out
}

/// O nível de um índice de taxa diária (CDI, em % ao dia): em cada data, o
/// acumulado das taxas **anteriores** a ela (a taxa de `d` rende de `d` para o
/// dia útil seguinte). Mais um ponto no dia seguinte à última taxa, com o
/// acumulado inteiro.
pub fn rate_levels(rates: &[(Date, Decimal)]) -> Levels {
    let mut out: Levels = Vec::with_capacity(rates.len() + 1);
    let mut factor = Decimal::ONE;
    for (date, rate) in rates {
        out.push((*date, factor));
        factor *= Decimal::ONE + rate / Decimal::ONE_HUNDRED;
    }
    if let Some((last, _)) = rates.last() {
        out.push((last.next_day().unwrap_or(*last), factor));
    }
    out
}

/// A data em que a comparação começa quando se "iguala o início": a mais
/// tardia entre o começo do período e o primeiro dado de cada série.
pub fn aligned_start(period_start: Date, starts: &[Date]) -> Date {
    starts.iter().copied().fold(period_start, Date::max)
}

/// O eixo das datas: as datas de todas as séries entre `from` e `to`, mais o
/// próprio `from` (a curva abre em 0% nele), afinado até [`MAX_POINTS`].
pub fn axis(all: &[&[(Date, Decimal)]], from: Date, to: Date) -> Vec<Date> {
    let mut dates: Vec<Date> = all
        .iter()
        .flat_map(|levels| levels.iter().map(|(d, _)| *d))
        .filter(|d| *d > from && *d <= to)
        .collect();
    dates.push(from);
    dates.sort_unstable();
    dates.dedup();
    thin(dates)
}

fn thin(dates: Vec<Date>) -> Vec<Date> {
    if dates.len() <= MAX_POINTS {
        return dates;
    }
    let last = dates.len() - 1;
    let step = last as f64 / (MAX_POINTS - 1) as f64;
    let mut picked: Vec<Date> = (0..MAX_POINTS)
        .map(|i| dates[((i as f64 * step).round() as usize).min(last)])
        .collect();
    picked.dedup();
    picked
}

/// Uma série normalizada: o retorno acumulado (fração) em cada data do eixo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    /// `None` antes de a série começar. Depois, o último nível conhecido vale
    /// (um ativo da B3 num feriado dos EUA fica onde estava).
    pub values: Vec<Option<Decimal>>,
    /// Onde a série passa a existir dentro da janela: a base, se há dado nela
    /// ou antes; senão, o primeiro dado.
    pub starts_on: Option<Date>,
}

/// Normaliza `levels` em 0% na data `base`. A base é o último nível em ou antes
/// de `base`; se a série só começa depois, a base é o primeiro nível (a linha
/// abre mais tarde, em 0% no próprio começo).
pub fn normalize(levels: &[(Date, Decimal)], base: Date, axis: &[Date]) -> Normalized {
    let Some((first_date, first_level)) = levels.first().copied() else {
        return Normalized {
            values: vec![None; axis.len()],
            starts_on: None,
        };
    };
    let (starts_on, base_level) = match value_at(levels, base) {
        Some(level) => (base, level),
        None => (first_date, first_level),
    };
    let values = axis
        .iter()
        .map(|date| {
            if *date < starts_on || base_level <= Decimal::ZERO {
                return None;
            }
            value_at(levels, *date).map(|level| level / base_level - Decimal::ONE)
        })
        .collect();
    Normalized {
        values,
        starts_on: Some(starts_on),
    }
}

/// O retorno de uma curva: o último valor conhecido.
pub fn last_value(values: &[Option<Decimal>]) -> Option<Decimal> {
    values.iter().rev().find_map(|v| *v)
}

/// O retorno de preço de 12 meses: o último fechamento sobre o de um ano
/// antes. `None` se a série não chega a um ano atrás (com folga de
/// [`BASE_SLACK_DAYS`]).
pub fn trailing_return(closes: &[(Date, Decimal)], today: Date) -> Option<Decimal> {
    let base_date = months_before(today, 12);
    let (first, _) = closes.first()?;
    if *first > base_date + Duration::days(BASE_SLACK_DAYS) {
        return None;
    }
    let base = value_at(closes, base_date).or_else(|| closes.first().map(|(_, c)| *c))?;
    let (_, last) = closes.last()?;
    (base > Decimal::ZERO).then(|| last / base - Decimal::ONE)
}

/// O rendimento de 12 meses: a soma dos proventos brutos com data ex nos 365
/// dias até `today` sobre o preço. `None` sem preço.
pub fn dividend_yield(
    dividends: &[(Date, Decimal)],
    price: Decimal,
    today: Date,
) -> Option<Decimal> {
    if price <= Decimal::ZERO {
        return None;
    }
    let since = today - Duration::days(365);
    let paid: Decimal = dividends
        .iter()
        .filter(|(ex, _)| *ex > since && *ex <= today)
        .map(|(_, amount)| *amount)
        .sum();
    Some(paid / price)
}

/// Fração para percentual, com 4 casas.
pub fn percent(fraction: Decimal) -> Decimal {
    (fraction * Decimal::ONE_HUNDRED).round_dp(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    #[test]
    fn periods_round_trip_and_start_before_today() {
        let today = date!(2026 - 11 - 27);
        for period in [
            Period::OneMonth,
            Period::ThreeMonths,
            Period::SixMonths,
            Period::YearToDate,
            Period::TwelveMonths,
            Period::ThreeYears,
            Period::FiveYears,
            Period::Max,
        ] {
            assert_eq!(Period::parse(period.as_str()), Some(period));
            assert!(period.start(today) < today);
        }
        assert_eq!(Period::parse("2y"), None);
        assert_eq!(Period::OneMonth.start(today), date!(2026 - 10 - 27));
        assert_eq!(Period::YearToDate.start(today), date!(2025 - 12 - 31));
        assert_eq!(Period::FiveYears.start(today), date!(2021 - 11 - 27));
    }

    #[test]
    fn tokens_carry_an_optional_class() {
        let plain = parse_token("petr4").unwrap();
        assert_eq!((plain.symbol.as_str(), plain.hint), ("PETR4", None));
        assert_eq!(plain.key(), "PETR4");

        let fund = parse_token("KNRI11:br_reits").unwrap();
        assert_eq!(fund.hint, Some((AssetClass::BrReits, true)));
        assert_eq!(fund.key(), "KNRI11:br_reits");

        let bdr = parse_token("AAPL34:intl_stocks:b3").unwrap();
        assert_eq!(bdr.hint, Some((AssetClass::IntlStocks, true)));
        assert_eq!(bdr.key(), "AAPL34:intl_stocks:b3");

        let us = parse_token("AAPL:intl_stocks").unwrap();
        assert_eq!(us.hint, Some((AssetClass::IntlStocks, false)));
        assert_eq!(us.key(), "AAPL:intl_stocks");

        assert_eq!(parse_token(""), None);
        assert_eq!(parse_token("AAPL:nada"), None);
        assert_eq!(parse_token("AAPL:crypto:foo"), None);
        assert_eq!(parse_token("AAPL:crypto:b3:x"), None);
        assert_eq!(parse_token("A B"), None);
    }

    #[test]
    fn a_hand_typed_symbol_gets_a_guess() {
        assert_eq!(guess_class("KNRI11"), (AssetClass::BrReits, true));
        assert_eq!(guess_class("PETR4"), (AssetClass::BrStocks, true));
        assert_eq!(guess_class("AAPL"), (AssetClass::IntlStocks, false));
        assert_eq!(guess_class("BTC"), (AssetClass::IntlStocks, false));
    }

    #[test]
    fn rows_know_where_they_apply() {
        assert!(Row::PriceToEarnings.applies_to(Family::Company));
        assert!(!Row::PriceToEarnings.applies_to(Family::Fund));
        assert!(Row::Vacancy.applies_to(Family::Fund));
        assert!(!Row::Vacancy.applies_to(Family::Other));
        assert!(Row::PriceToBook.applies_to(Family::Fund));
        assert!(!Row::DividendYield12m.applies_to(Family::Other));
        assert!(Row::Price.applies_to(Family::Other));
        // Todas as linhas têm lugar na tabela e nenhuma se repete.
        let mut seen = std::collections::HashSet::new();
        assert!(Row::ALL.iter().all(|r| seen.insert(*r)));
    }

    #[test]
    fn marks_the_extreme_of_the_direction_within_the_family() {
        let c = Family::Company;
        let f = Family::Fund;
        let cells = [
            (c, Some(dec!(10))),
            (c, Some(dec!(4))),
            (f, Some(dec!(1))),
            (c, None),
            (c, Some(dec!(4))),
        ];
        let lower = marks(&cells, Direction::Lower);
        assert_eq!(
            lower,
            vec![None, Some(Mark::Min), None, None, Some(Mark::Min)]
        );
        let higher = marks(&cells, Direction::Higher);
        assert_eq!(higher, vec![Some(Mark::Max), None, None, None, None]);
        assert_eq!(marks(&cells, Direction::None), vec![None; 5]);
    }

    #[test]
    fn a_lone_value_is_not_an_extreme() {
        let cells = [
            (Family::Company, Some(dec!(3))),
            (Family::Fund, Some(dec!(1))),
        ];
        assert_eq!(marks(&cells, Direction::Higher), vec![None, None]);
    }

    #[test]
    fn splits_are_undone_for_the_days_before_them() {
        let closes = vec![
            (date!(2026 - 03 - 03), dec!(100)),
            (date!(2026 - 03 - 04), dec!(102)),
            (date!(2026 - 03 - 05), dec!(52)),
        ];
        // 2:1 em 05/03: antes dele o preço real vale metade na base de hoje.
        let adjusted = adjust_for_splits(&closes, &[(date!(2026 - 03 - 05), dec!(2))]);
        assert_eq!(
            adjusted,
            vec![
                (date!(2026 - 03 - 03), dec!(50)),
                (date!(2026 - 03 - 04), dec!(51)),
                (date!(2026 - 03 - 05), dec!(52)),
            ]
        );
        assert_eq!(adjust_for_splits(&closes, &[]), closes);
    }

    #[test]
    fn dividends_are_reinvested_on_the_ex_date() {
        let closes = vec![
            (date!(2026 - 03 - 02), dec!(10)),
            (date!(2026 - 03 - 03), dec!(10)),
            (date!(2026 - 03 - 04), dec!(9.5)),
            (date!(2026 - 03 - 05), dec!(9.5)),
        ];
        // R$ 0,50 com data ex em 04/03: o preço cai 0,50 e o retorno total fica em 0%.
        let levels = total_return_levels(&closes, &[(date!(2026 - 03 - 04), dec!(0.5))]);
        assert_eq!(levels.last().map(|(_, l)| *l), Some(dec!(10)));
        assert_eq!(levels.first().map(|(_, l)| *l), Some(dec!(10)));
        // Sem provento, é o preço.
        let price_only = total_return_levels(&closes, &[]);
        assert_eq!(price_only.last().map(|(_, l)| *l), Some(dec!(9.5)));
        // Provento antes do primeiro fechamento ou depois do último não entra.
        let outside = total_return_levels(
            &closes,
            &[
                (date!(2026 - 03 - 01), dec!(1)),
                (date!(2026 - 03 - 09), dec!(1)),
            ],
        );
        assert_eq!(outside, price_only);
    }

    #[test]
    fn a_non_trading_ex_date_is_paid_on_the_next_close() {
        let closes = vec![
            (date!(2026 - 03 - 06), dec!(10)), // sexta
            (date!(2026 - 03 - 09), dec!(10)), // segunda
        ];
        let levels = total_return_levels(&closes, &[(date!(2026 - 03 - 07), dec!(1))]);
        assert_eq!(levels.last().map(|(_, l)| *l), Some(dec!(11)));
    }

    #[test]
    fn the_daily_rate_curve_accumulates_the_previous_rates() {
        let rates = vec![
            (date!(2026 - 03 - 02), dec!(1)),
            (date!(2026 - 03 - 03), dec!(1)),
        ];
        let levels = rate_levels(&rates);
        assert_eq!(levels[0], (date!(2026 - 03 - 02), dec!(1)));
        assert_eq!(levels[1], (date!(2026 - 03 - 03), dec!(1.01)));
        assert_eq!(levels[2], (date!(2026 - 03 - 04), dec!(1.0201)));
        assert!(rate_levels(&[]).is_empty());
    }

    #[test]
    fn the_aligned_start_is_the_latest_first_date() {
        let from = date!(2026 - 01 - 01);
        assert_eq!(
            aligned_start(from, &[date!(2025 - 05 - 01), date!(2026 - 03 - 10)]),
            date!(2026 - 03 - 10)
        );
        assert_eq!(aligned_start(from, &[date!(2020 - 01 - 01)]), from);
        assert_eq!(aligned_start(from, &[]), from);
    }

    fn levels() -> Levels {
        vec![
            (date!(2026 - 03 - 02), dec!(100)),
            (date!(2026 - 03 - 03), dec!(110)),
            (date!(2026 - 03 - 05), dec!(99)),
        ]
    }

    #[test]
    fn a_curve_opens_at_zero_on_the_base_and_carries_the_last_level() {
        let axis = vec![
            date!(2026 - 03 - 02),
            date!(2026 - 03 - 03),
            date!(2026 - 03 - 04),
            date!(2026 - 03 - 05),
        ];
        let n = normalize(&levels(), date!(2026 - 03 - 02), &axis);
        assert_eq!(n.starts_on, Some(date!(2026 - 03 - 02)));
        assert_eq!(
            n.values,
            vec![
                Some(dec!(0)),
                Some(dec!(0.1)),
                Some(dec!(0.1)),
                Some(dec!(-0.01))
            ]
        );
        assert_eq!(last_value(&n.values), Some(dec!(-0.01)));
    }

    #[test]
    fn a_series_that_starts_late_opens_at_its_own_first_date() {
        let axis = vec![
            date!(2026 - 03 - 01),
            date!(2026 - 03 - 02),
            date!(2026 - 03 - 03),
        ];
        let n = normalize(&levels(), date!(2026 - 03 - 01), &axis);
        assert_eq!(n.starts_on, Some(date!(2026 - 03 - 02)));
        assert_eq!(n.values, vec![None, Some(dec!(0)), Some(dec!(0.1))]);
        let empty = normalize(&[], date!(2026 - 03 - 01), &axis);
        assert_eq!(empty.values, vec![None; 3]);
        assert_eq!(empty.starts_on, None);
    }

    #[test]
    fn the_axis_holds_every_date_in_the_window_and_the_start() {
        let a = vec![
            (date!(2026 - 03 - 02), dec!(1)),
            (date!(2026 - 03 - 05), dec!(1)),
        ];
        let b = vec![
            (date!(2026 - 03 - 03), dec!(1)),
            (date!(2026 - 03 - 05), dec!(1)),
        ];
        let axis = axis(&[&a, &b], date!(2026 - 03 - 01), date!(2026 - 03 - 05));
        assert_eq!(
            axis,
            vec![
                date!(2026 - 03 - 01),
                date!(2026 - 03 - 02),
                date!(2026 - 03 - 03),
                date!(2026 - 03 - 05)
            ]
        );
    }

    #[test]
    fn a_long_axis_is_thinned_and_keeps_both_ends() {
        let first = date!(2000 - 01 - 01);
        let dates: Vec<Date> = (0..5_000).map(|n| first + Duration::days(n)).collect();
        let thinned = thin(dates.clone());
        assert!(thinned.len() <= MAX_POINTS);
        assert_eq!(thinned.first(), dates.first());
        assert_eq!(thinned.last(), dates.last());
        assert!(thinned.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn the_trailing_return_needs_a_year_of_history() {
        let today = date!(2026 - 11 - 27);
        let closes = vec![
            (date!(2025 - 11 - 26), dec!(10)),
            (date!(2026 - 06 - 01), dec!(11)),
            (date!(2026 - 11 - 27), dec!(12)),
        ];
        assert_eq!(trailing_return(&closes, today), Some(dec!(0.2)));
        assert_eq!(trailing_return(&closes[1..], today), None);
        assert_eq!(trailing_return(&[], today), None);
    }

    #[test]
    fn the_yield_counts_the_last_365_days_of_ex_dates() {
        let today = date!(2026 - 11 - 27);
        let paid = vec![
            (date!(2025 - 11 - 20), dec!(9)), // fora da janela
            (date!(2026 - 02 - 10), dec!(0.5)),
            (date!(2026 - 08 - 10), dec!(0.5)),
            (date!(2026 - 12 - 01), dec!(7)), // futuro
        ];
        assert_eq!(dividend_yield(&paid, dec!(20), today), Some(dec!(0.05)));
        assert_eq!(dividend_yield(&paid, dec!(0), today), None);
        assert_eq!(dividend_yield(&[], dec!(20), today), Some(dec!(0)));
    }

    #[test]
    fn percent_keeps_four_places() {
        assert_eq!(percent(dec!(0.123456)), dec!(12.3456));
        assert_eq!(percent(dec!(-0.5)), dec!(-50));
    }

    proptest! {
        #[test]
        fn a_normalized_curve_is_zero_on_its_base(
            closes in prop::collection::vec(1u32..100_000, 2..40),
        ) {
            let start = date!(2026 - 01 - 01);
            let levels: Levels = closes
                .iter()
                .enumerate()
                .map(|(n, c)| (start + Duration::days(n as i64), Decimal::new(i64::from(*c), 2)))
                .collect();
            let end = levels.last().map(|(d, _)| *d).unwrap();
            let axis = axis(&[&levels], start, end);
            let n = normalize(&levels, start, &axis);
            prop_assert_eq!(n.values[0], Some(Decimal::ZERO));
            prop_assert!(n.values.iter().all(Option::is_some));
        }

        #[test]
        fn the_mark_always_lands_on_a_known_extreme(
            values in prop::collection::vec(prop::option::of(-1_000i32..1_000), 2..8),
            higher in any::<bool>(),
        ) {
            let cells: Vec<(Family, Option<Decimal>)> = values
                .iter()
                .map(|v| (Family::Company, v.map(Decimal::from)))
                .collect();
            let direction = if higher { Direction::Higher } else { Direction::Lower };
            let result = marks(&cells, direction);
            let known: Vec<Decimal> = cells.iter().filter_map(|(_, v)| *v).collect();
            for (cell, mark) in cells.iter().zip(&result) {
                if let Some(mark) = mark {
                    let value = cell.1.unwrap();
                    let extreme = if *mark == Mark::Max {
                        known.iter().copied().max()
                    } else {
                        known.iter().copied().min()
                    };
                    prop_assert_eq!(Some(value), extreme);
                    prop_assert!(known.len() >= 2);
                }
            }
            if known.len() >= 2 {
                prop_assert!(result.iter().any(Option::is_some));
            }
        }

        #[test]
        fn dividends_never_lower_the_total_return(
            closes in prop::collection::vec(100u32..10_000, 2..20),
            payout in 0u32..500,
        ) {
            let start = date!(2026 - 01 - 01);
            let series: Levels = closes
                .iter()
                .enumerate()
                .map(|(n, c)| (start + Duration::days(n as i64), Decimal::new(i64::from(*c), 2)))
                .collect();
            let dividends = vec![(start + Duration::days(1), Decimal::new(i64::from(payout), 2))];
            let with = total_return_levels(&series, &dividends);
            let without = total_return_levels(&series, &[]);
            prop_assert!(with.last().unwrap().1 >= without.last().unwrap().1);
        }
    }
}
