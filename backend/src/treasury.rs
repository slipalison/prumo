//! A tabela de preços do Tesouro Direto e o que o Prumo faz com ela.
//!
//! A fonte é o conjunto "Taxas dos Títulos Ofertados pelo Tesouro Direto" do
//! Tesouro Transparente (CKAN, dados abertos, sem chave): um CSV com o preço e a
//! taxa de cada título em cada dia, de 2004 até a última divulgação — uns 15 MB.
//! O Prumo lê o arquivo em fluxo, linha a linha, e guarda só o mais recente de
//! cada título que ainda não venceu: a cotação de resgate (`PU Venda Manha`, o
//! preço a que o Tesouro recompra o título da pessoa) e a taxa de compra
//! (`Taxa Compra Manha`). Da taxa dos títulos IPCA+ sai o juro real de
//! referência dos FIIs de tijolo ([`crate::domain::market_rates`]).
//!
//! O Tesouro divulga uma vez por dia útil e o preço não muda fora disso, então
//! o Prumo baixa a tabela no máximo uma vez a cada [`MAX_AGE`]. A explicação e
//! os limites estão em `docs/tesouro.md`.
//!
//! O histórico de cada título (o preço de todos os dias, para a série
//! retroativa do Desempenho) sai do mesmo arquivo: ver [`history`].

pub mod history;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use time::macros::offset;
use time::{Date, OffsetDateTime, Time, UtcOffset};

use crate::domain::asset_class::Market;
use crate::domain::market_rates::{is_real_rate_bond, pick_real_rate, RealRate};
use crate::domain::treasury::{unique_names, Bond, BondKind};
use crate::live::LiveQuotes;
use crate::quotes::Quote;
use crate::store::{Store, StoreError};

/// O endereço do site da fonte.
pub const DEFAULT_BASE: &str = "https://www.tesourotransparente.gov.br";

/// O conjunto de dados no CKAN. O id não muda quando o arquivo é reenviado,
/// ao contrário do endereço do CSV.
pub const DATASET_ID: &str = "df56aa42-484a-4a59-8184-7676580c81e3";

/// Onde o CSV estava quando foi conferido (25/11/2026). Só vale se a consulta
/// ao CKAN falhar: o endereço certo vem do `package_show`.
pub const KNOWN_CSV_PATH: &str = "/ckan/dataset/df56aa42-484a-4a59-8184-7676580c81e3/resource/796d2059-14e9-44e3-80c9-2d9e30b405c1/download/precotaxatesourodireto.csv";

/// Tempo para baixar o arquivo inteiro.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
/// Tempo para o CKAN dizer onde está o CSV.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);

/// De quanto em quanto tempo a rodada confere se a tabela está velha.
pub const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// Idade da última gravação a partir da qual a tabela é baixada de novo: um
/// pouco menos de um dia, para o horário de download não escorregar a cada dia.
pub const MAX_AGE: Duration = Duration::from_secs(20 * 60 * 60);

/// Brasília (UTC−3, sem horário de verão desde 2019): a data-base do Tesouro é
/// a do dia útil em Brasília.
const BRASILIA: UtcOffset = offset!(-3);

/// O preço de resgate de um título na data-base da tabela.
#[derive(Debug, Clone, PartialEq)]
pub struct BondPrice {
    pub bond: Bond,
    /// `PU Venda Manha`: quanto o Tesouro paga por um título inteiro.
    pub redemption_price: Decimal,
    pub base_date: Date,
    /// `Taxa Compra Manha`, em % ao ano: a taxa de quem compra o título no dia.
    /// Só existe quando o título está à venda (`PU Compra Manha` maior que
    /// zero) e as duas colunas estão no arquivo.
    pub buy_rate: Option<Decimal>,
}

impl BondPrice {
    /// A taxa do dia como juro real, nos títulos IPCA+.
    pub fn real_rate(&self) -> Option<RealRate> {
        let percent = self.buy_rate?;
        is_real_rate_bond(self.bond.kind).then_some(RealRate {
            bond: self.bond,
            percent,
            base_date: self.base_date,
        })
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FeedError {
    #[error("fonte do tesouro indisponivel: {0}")]
    Unavailable(String),
    #[error("tabela do tesouro fora do formato esperado: {0}")]
    Malformed(String),
}

/// De onde vêm os preços. Fica atrás de um trait para os testes não dependerem
/// da rede e para a troca de fonte não tocar o resto.
#[async_trait]
pub trait TreasuryFeed: Send + Sync {
    /// O preço de resgate mais recente de cada título que ainda não venceu em
    /// `today`.
    async fn latest_prices(&self, today: Date) -> Result<Vec<BondPrice>, FeedError>;
}

// ----------------------------------------------------------------- leitura --

/// As colunas que o Prumo lê, achadas pelo nome do cabeçalho: se o Tesouro
/// reordenar ou acrescentar colunas, a leitura segue valendo.
#[derive(Debug, Clone, Copy)]
struct Columns {
    title: usize,
    maturity: usize,
    base_date: usize,
    redemption: usize,
    /// A taxa e o preço de compra: opcionais (sem eles, só não há taxa).
    buy_rate: Option<(usize, usize)>,
    widest: usize,
}

/// Cabeçalho sem acento, em minúsculas e sem espaço nas pontas.
fn fold(header: &str) -> String {
    header
        .trim()
        .trim_start_matches('\u{feff}')
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

impl Columns {
    fn from_header(line: &str) -> Result<Self, FeedError> {
        let names: Vec<String> = line.split(';').map(fold).collect();
        let find = |wanted: &str| {
            names.iter().position(|n| n == wanted).ok_or_else(|| {
                FeedError::Malformed(format!("coluna \"{wanted}\" ausente do cabecalho"))
            })
        };
        let title = find("tipo titulo")?;
        let maturity = find("data vencimento")?;
        let base_date = find("data base")?;
        let redemption = find("pu venda manha")?;
        let optional = |wanted: &str| names.iter().position(|n| n == wanted);
        let buy_rate = optional("taxa compra manha").zip(optional("pu compra manha"));
        Ok(Self {
            title,
            maturity,
            base_date,
            redemption,
            buy_rate,
            widest: [title, maturity, base_date, redemption]
                .into_iter()
                .max()
                .unwrap_or(0),
        })
    }
}

/// `19/11/2026` → data.
fn parse_date(text: &str) -> Option<Date> {
    let mut parts = text.trim().split('/');
    let (day, month, year) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Date::from_calendar_date(
        year.parse().ok()?,
        time::Month::try_from(month.parse::<u8>().ok()?).ok()?,
        day.parse().ok()?,
    )
    .ok()
}

/// `20332,84` → decimal (vírgula decimal, sem separador de milhar).
fn parse_price(text: &str) -> Option<Decimal> {
    text.trim().replace(',', ".").parse().ok()
}

/// O resultado da leitura: os preços e o que ficou de fora.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub prices: Vec<BondPrice>,
    /// Nomes de tipo que o Prumo não conhece (um título novo do Tesouro).
    pub unknown_titles: BTreeSet<String>,
    /// Linhas com data ou preço que não deu para ler.
    pub unreadable_rows: usize,
}

/// Lê a tabela linha a linha, guardando só o preço mais recente de cada
/// título que ainda não venceu. A ordem das linhas não importa (hoje o arquivo
/// vem do mais novo para o mais velho, mas isso não é contrato).
pub struct PriceBoard {
    today: Date,
    columns: Option<Columns>,
    /// Título → (data-base, preço de resgate, taxa de compra).
    latest: BTreeMap<Bond, (Date, Decimal, Option<Decimal>)>,
    unknown_titles: BTreeSet<String>,
    unreadable_rows: usize,
}

impl PriceBoard {
    pub fn new(today: Date) -> Self {
        Self {
            today,
            columns: None,
            latest: BTreeMap::new(),
            unknown_titles: BTreeSet::new(),
            unreadable_rows: 0,
        }
    }

    /// Uma linha do arquivo, com ou sem o fim de linha. A primeira não vazia é
    /// o cabeçalho.
    pub fn push_line(&mut self, line: &str) -> Result<(), FeedError> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return Ok(());
        }
        let Some(columns) = self.columns else {
            self.columns = Some(Columns::from_header(line)?);
            return Ok(());
        };
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() <= columns.widest {
            self.unreadable_rows += 1;
            return Ok(());
        }
        let Some(kind) = BondKind::from_title(fields[columns.title]) else {
            self.unknown_titles
                .insert(fields[columns.title].trim().to_owned());
            return Ok(());
        };
        let (Some(maturity), Some(base_date), Some(price)) = (
            parse_date(fields[columns.maturity]),
            parse_date(fields[columns.base_date]),
            parse_price(fields[columns.redemption]),
        ) else {
            self.unreadable_rows += 1;
            return Ok(());
        };
        // Vencido não tem o que cotar. Preço zero é título fora de venda no
        // dia: a linha existe, mas sem preço de resgate.
        if maturity < self.today || price <= Decimal::ZERO {
            return Ok(());
        }
        // A taxa de compra só vale no dia em que o título está à venda.
        let buy_rate = columns.buy_rate.and_then(|(rate, buy_price)| {
            let offered = fields
                .get(buy_price)
                .and_then(|f| parse_price(f))
                .is_some_and(|p| p > Decimal::ZERO);
            offered
                .then(|| fields.get(rate).and_then(|f| parse_price(f)))
                .flatten()
        });
        let bond = Bond::new(kind, maturity);
        match self.latest.get(&bond) {
            Some((date, _, _)) if *date >= base_date => {}
            _ => {
                self.latest.insert(bond, (base_date, price, buy_rate));
            }
        }
        Ok(())
    }

    /// O fim do arquivo. Sem nenhum título é erro: um arquivo que não trouxe
    /// preço algum (formato mudou, página de erro) não pode passar por vazio e
    /// apagar o catálogo.
    pub fn finish(self) -> Result<Parsed, FeedError> {
        if self.columns.is_none() {
            return Err(FeedError::Malformed("arquivo vazio".into()));
        }
        if self.latest.is_empty() {
            return Err(FeedError::Malformed(format!(
                "nenhum titulo vigente com preco ({} linhas ilegiveis)",
                self.unreadable_rows
            )));
        }
        Ok(Parsed {
            prices: self
                .latest
                .into_iter()
                .map(
                    |(bond, (base_date, redemption_price, buy_rate))| BondPrice {
                        bond,
                        redemption_price,
                        base_date,
                        buy_rate,
                    },
                )
                .collect(),
            unknown_titles: self.unknown_titles,
            unreadable_rows: self.unreadable_rows,
        })
    }
}

/// Lê um CSV inteiro já em memória. Só os testes e as amostras usam; o
/// download lê em fluxo.
pub fn parse_csv(csv: &str, today: Date) -> Result<Parsed, FeedError> {
    let mut board = PriceBoard::new(today);
    for line in csv.lines() {
        board.push_line(line)?;
    }
    board.finish()
}

// ---------------------------------------------------------------- download --

#[derive(Deserialize)]
struct PackageShow {
    result: PackageResult,
}

#[derive(Deserialize)]
struct PackageResult {
    #[serde(default)]
    resources: Vec<PackageResource>,
}

#[derive(Deserialize)]
struct PackageResource {
    #[serde(default)]
    format: String,
    #[serde(default)]
    url: String,
}

/// A fonte de verdade: o CSV do Tesouro Transparente.
pub struct TransparenteFeed {
    http: reqwest::Client,
    base: String,
}

impl TransparenteFeed {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, DEFAULT_BASE)
    }

    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    /// O caminho do CSV, como o CKAN o informa hoje. Só o caminho vale: o
    /// servidor é sempre o configurado, o que o CKAN mandar não muda para onde
    /// o Prumo vai.
    async fn discover_csv_path(&self) -> Result<String, FeedError> {
        let package: PackageShow = self
            .http
            .get(format!("{}/ckan/api/3/action/package_show", self.base))
            .query(&[("id", DATASET_ID)])
            .timeout(DISCOVERY_TIMEOUT)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| FeedError::Unavailable(e.to_string()))?
            .json()
            .await
            .map_err(|e| FeedError::Malformed(e.to_string()))?;
        package
            .result
            .resources
            .iter()
            .filter(|r| r.format.eq_ignore_ascii_case("csv"))
            .find_map(|r| reqwest::Url::parse(&r.url).ok())
            .map(|url| url.path().to_owned())
            .ok_or_else(|| FeedError::Malformed("o CKAN nao lista um recurso CSV".into()))
    }

    /// O caminho do CSV: o que o CKAN informa ou, se ele falhar, o conhecido.
    async fn csv_path(&self) -> String {
        match self.discover_csv_path().await {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(error = %error, "ckan nao disse onde esta o csv do tesouro, usando o endereco conhecido");
                KNOWN_CSV_PATH.to_owned()
            }
        }
    }

    /// Baixa o CSV em fluxo e entrega cada linha a `line`, sem guardar o
    /// arquivo: são uns 15 MB, e quem lê guarda só o que precisa.
    async fn for_each_line(
        &self,
        path: &str,
        mut line: impl FnMut(&str) -> Result<(), FeedError>,
    ) -> Result<(), FeedError> {
        let mut response = self
            .http
            .get(format!("{}{path}", self.base))
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| FeedError::Unavailable(e.to_string()))?;
        // O arquivo chega em pedaços que não respeitam o fim de linha: o que
        // sobra do pedaço espera o próximo.
        let mut pending: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| FeedError::Unavailable(e.to_string()))?
        {
            pending.extend_from_slice(&chunk);
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                let bytes: Vec<u8> = pending.drain(..=end).collect();
                line(&String::from_utf8_lossy(&bytes))?;
            }
        }
        if !pending.is_empty() {
            line(&String::from_utf8_lossy(&pending))?;
        }
        Ok(())
    }

    async fn read_csv(&self, path: &str, today: Date) -> Result<Parsed, FeedError> {
        let mut board = PriceBoard::new(today);
        self.for_each_line(path, |line| board.push_line(line))
            .await?;
        board.finish()
    }
}

#[async_trait]
impl TreasuryFeed for TransparenteFeed {
    async fn latest_prices(&self, today: Date) -> Result<Vec<BondPrice>, FeedError> {
        let path = self.csv_path().await;
        let parsed = self.read_csv(&path, today).await?;
        if !parsed.unknown_titles.is_empty() {
            tracing::warn!(titles = ?parsed.unknown_titles, "tipos de titulo do tesouro que o prumo nao conhece");
        }
        if parsed.unreadable_rows > 0 {
            tracing::warn!(
                rows = parsed.unreadable_rows,
                "linhas da tabela do tesouro ilegiveis"
            );
        }
        Ok(parsed.prices)
    }
}

// ------------------------------------------------------------- sincronia --

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Feed(#[from] FeedError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// O que uma rodada gravou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncReport {
    pub bonds: usize,
    pub pruned: u64,
    /// O juro real de referência que a rodada gravou, se a tabela trouxe um
    /// título IPCA+ com taxa.
    pub real_rate: Option<RealRate>,
}

/// A data de hoje em Brasília.
pub fn today_in_brasilia(now: OffsetDateTime) -> Date {
    now.to_offset(BRASILIA).date()
}

/// A data-base como instante: a meia-noite em Brasília, que é o que a tela
/// mostra como "de quando é o preço".
fn base_instant(date: Date) -> OffsetDateTime {
    date.with_time(Time::MIDNIGHT).assume_offset(BRASILIA)
}

/// Baixa a tabela e grava a cotação de cada título em `quotes`.
pub struct TreasurySync {
    store: Store,
    feed: Arc<dyn TreasuryFeed>,
    live: Arc<LiveQuotes>,
}

impl TreasurySync {
    pub fn new(store: Store, feed: Arc<dyn TreasuryFeed>, live: Arc<LiveQuotes>) -> Self {
        Self { store, feed, live }
    }

    /// Baixa a tabela agora e grava: preço de cada título vigente, apaga os
    /// que saíram de oferta (menos os que alguma carteira guarda) e avisa quem
    /// acompanha a carteira ao vivo.
    pub async fn sync(&self, now: OffsetDateTime) -> Result<SyncReport, SyncError> {
        let prices = self.feed.latest_prices(today_in_brasilia(now)).await?;
        let bonds: Vec<Bond> = prices.iter().map(|p| p.bond).collect();
        let names = unique_names(&bonds);
        let mut symbols = Vec::with_capacity(prices.len());
        for (price, name) in prices.iter().zip(names) {
            let symbol = price.bond.symbol();
            self.store
                .upsert_quote(
                    &Quote {
                        symbol: symbol.clone(),
                        price: price.redemption_price,
                        currency: "BRL".to_owned(),
                        name: Some(name),
                        traded_at: Some(base_instant(price.base_date)),
                    },
                    now,
                )
                .await?;
            symbols.push(symbol);
        }
        // O preço do dia de cada título que alguma carteira guarda vai também
        // para o histórico: a série do Desempenho anda para a frente sem baixar
        // a tabela de novo (o passado vem de [`history`], uma vez só).
        let held: BTreeSet<String> = self
            .store
            .tracked_symbols()
            .await?
            .into_iter()
            .filter(|(market, _)| *market == Market::Treasury)
            .map(|(_, symbol)| symbol)
            .collect();
        // O caminho comum de gravação: o preço publicado de hoje entra, e o de um
        // dia recente que a fonte corrigiu é acertado (com a série refeita).
        for price in prices.iter().filter(|p| held.contains(&p.bond.symbol())) {
            self.store
                .record_prices(
                    &price.bond.symbol(),
                    &vec![(price.base_date, price.redemption_price)],
                    now,
                )
                .await?;
        }
        let pruned = self.store.prune_treasury(&symbols).await?;
        // O juro real de referência dos FIIs de tijolo: a NTN-B mais perto de
        // 10 anos, do dia mais novo. Tabela sem IPCA+ com taxa: o de ontem fica.
        let rates: Vec<RealRate> = prices.iter().filter_map(BondPrice::real_rate).collect();
        let real_rate = pick_real_rate(&rates);
        if let Some(rate) = &real_rate {
            self.store.save_real_rate(rate, now).await?;
        }
        let report = SyncReport {
            bonds: symbols.len(),
            pruned,
            real_rate,
        };
        self.live.announce(symbols.into_iter().collect());
        Ok(report)
    }

    /// Baixa só se a última gravação é velha (ou não existe), ou se ainda não
    /// há juro real de referência guardado (o primeiro deploy com ele, ou uma
    /// tabela que veio sem IPCA+): sem ele, o alvo do FII de tijolo esperaria
    /// até 20 horas. `None` quando a tabela estava em dia e nada foi feito.
    pub async fn sync_if_stale(
        &self,
        now: OffsetDateTime,
        max_age: Duration,
    ) -> Result<Option<SyncReport>, SyncError> {
        if let Some(last) = self.store.last_treasury_fetch().await? {
            if now - last < max_age && self.store.real_rate().await?.is_some() {
                return Ok(None);
            }
        }
        self.sync(now).await.map(Some)
    }

    /// A rodada periódica, até o processo encerrar. Falhar não derruba nada: o
    /// preço de ontem continua valendo e a próxima conferência tenta de novo.
    pub fn spawn(
        self: Arc<Self>,
        check_every: Duration,
        max_age: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(check_every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match self.sync_if_stale(OffsetDateTime::now_utc(), max_age).await {
                    Ok(Some(report)) => {
                        tracing::info!(
                            bonds = report.bonds,
                            pruned = report.pruned,
                            "tabela do tesouro atualizada"
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(error = %error, "tabela do tesouro nao atualizada");
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::{date, datetime};

    const TODAY: Date = date!(2026 - 11 - 25);
    const HEADER: &str = "Tipo Titulo;Data Vencimento;Data Base;Taxa Compra Manha;Taxa Venda Manha;PU Compra Manha;PU Venda Manha;PU Base Manha";

    fn csv(rows: &[&str]) -> String {
        std::iter::once(HEADER)
            .chain(rows.iter().copied())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn price_of(parsed: &Parsed, kind: BondKind, maturity: Date) -> Option<(Decimal, Date)> {
        parsed
            .prices
            .iter()
            .find(|p| p.bond == Bond::new(kind, maturity))
            .map(|p| (p.redemption_price, p.base_date))
    }

    #[test]
    fn reads_the_redemption_price_not_the_purchase_price() {
        let parsed = parse_csv(
            &csv(&["Tesouro Selic;01/03/2029;19/11/2026;0,05;0,06;20320,65;20305,65;20305,65"]),
            TODAY,
        )
        .unwrap();
        assert_eq!(
            price_of(&parsed, BondKind::Selic, date!(2029 - 03 - 01)),
            Some((dec!(20305.65), date!(2026 - 11 - 19)))
        );
        assert_eq!(parsed.prices.len(), 1);
    }

    #[test]
    fn keeps_the_buy_rate_of_the_newest_row_when_the_bond_is_on_sale() {
        let parsed = parse_csv(
            &csv(&[
                "Tesouro IPCA+ com Juros Semestrais;15/05/2037;26/11/2026;7,60;7,72;4275,56;4237,24;4237,24",
                "Tesouro IPCA+ com Juros Semestrais;15/05/2037;25/11/2026;7,71;7,83;4240,26;4202,32;4202,32",
                // Fora de venda para compra no dia (PU de compra zerado): sem taxa.
                "Tesouro IPCA+;15/05/2035;26/11/2026;7,73;7,85;0,00;2531,74;2531,74",
                "Tesouro Selic;01/03/2029;26/11/2026;0,05;0,06;20320,65;20305,65;20305,65",
            ]),
            TODAY,
        )
        .unwrap();
        let find = |kind, maturity| {
            parsed
                .prices
                .iter()
                .find(|p| p.bond == Bond::new(kind, maturity))
                .unwrap()
        };
        let ntnb = find(BondKind::IpcaPlusWithCoupons, date!(2037 - 05 - 15));
        assert_eq!(ntnb.buy_rate, Some(dec!(7.60)));
        assert_eq!(
            ntnb.real_rate(),
            Some(RealRate {
                bond: ntnb.bond,
                percent: dec!(7.60),
                base_date: date!(2026 - 11 - 26)
            })
        );
        assert_eq!(
            find(BondKind::IpcaPlus, date!(2035 - 05 - 15)).buy_rate,
            None
        );
        // Selic tem taxa, mas não é juro real.
        let selic = find(BondKind::Selic, date!(2029 - 03 - 01));
        assert_eq!(selic.buy_rate, Some(dec!(0.05)));
        assert_eq!(selic.real_rate(), None);
        let rates: Vec<RealRate> = parsed
            .prices
            .iter()
            .filter_map(BondPrice::real_rate)
            .collect();
        assert_eq!(pick_real_rate(&rates).map(|r| r.bond), Some(ntnb.bond));
    }

    #[test]
    fn without_the_buy_columns_the_price_is_still_read() {
        let text = "Tipo Titulo;Data Vencimento;Data Base;PU Venda Manha
                    Tesouro IPCA+;15/05/2035;19/11/2026;2500,95
";
        let parsed = parse_csv(text, TODAY).unwrap();
        assert_eq!(parsed.prices.len(), 1);
        assert_eq!(parsed.prices[0].buy_rate, None);
        assert_eq!(parsed.prices[0].redemption_price, dec!(2500.95));
    }

    #[test]
    fn keeps_the_newest_row_of_each_bond_in_any_order() {
        let newest_first = csv(&[
            "Tesouro IPCA+;15/05/2035;19/11/2026;7,86;7,98;2526,53;2500,95;2500,95",
            "Tesouro IPCA+;15/05/2035;18/11/2026;7,77;7,89;2542,31;2517,51;2517,51",
        ]);
        let oldest_first = csv(&[
            "Tesouro IPCA+;15/05/2035;18/11/2026;7,77;7,89;2542,31;2517,51;2517,51",
            "Tesouro IPCA+;15/05/2035;19/11/2026;7,86;7,98;2526,53;2500,95;2500,95",
        ]);
        for text in [newest_first, oldest_first] {
            let parsed = parse_csv(&text, TODAY).unwrap();
            assert_eq!(
                price_of(&parsed, BondKind::IpcaPlus, date!(2035 - 05 - 15)),
                Some((dec!(2500.95), date!(2026 - 11 - 19)))
            );
        }
    }

    #[test]
    fn skips_matured_bonds_and_rows_without_a_redemption_price() {
        let parsed = parse_csv(
            &csv(&[
                // Venceu em 2020.
                "Tesouro IPCA+ com Juros Semestrais;15/08/2020;14/08/2020;-1,78;-1,66;3400,00;3407,83;3407,83",
                // Venceu ontem; vence hoje ainda vale.
                "Tesouro Prefixado;24/11/2026;19/11/2026;13,00;13,10;999,00;998,00;998,00",
                "Tesouro Prefixado;25/11/2026;19/11/2026;13,00;13,10;999,00;997,00;997,00",
                // Fora de venda no dia: preço de resgate zerado.
                "Tesouro Educa+;15/12/2037;20/11/2026;7,80;7,92;0,00;0,00;2502,15",
                "Tesouro Educa+;15/12/2037;19/11/2026;7,80;7,92;2527,68;2502,15;2502,15",
            ]),
            TODAY,
        )
        .unwrap();
        assert_eq!(parsed.prices.len(), 2);
        assert_eq!(
            price_of(&parsed, BondKind::Prefixed, date!(2026 - 11 - 25)),
            Some((dec!(997), date!(2026 - 11 - 19)))
        );
        // A linha zerada de 20/11 não esconde o último preço de verdade.
        assert_eq!(
            price_of(&parsed, BondKind::EducaPlus, date!(2037 - 12 - 15)),
            Some((dec!(2502.15), date!(2026 - 11 - 19)))
        );
    }

    #[test]
    fn finds_columns_by_name_and_tolerates_bom_crlf_accents_and_extra_columns() {
        let text = "\u{feff}Extra;Tipo Título;PU Venda Manhã;Data Base;Data Vencimento\r\n\
                    x;Tesouro Renda+ Aposentadoria Extra;1974,09;19/11/2026;15/12/2049\r\n";
        let parsed = parse_csv(text, TODAY).unwrap();
        assert_eq!(
            price_of(&parsed, BondKind::RendaPlus, date!(2049 - 12 - 15)),
            Some((dec!(1974.09), date!(2026 - 11 - 19)))
        );
    }

    #[test]
    fn counts_what_it_could_not_read_and_names_unknown_types() {
        let parsed = parse_csv(
            &csv(&[
                "Tesouro Selic;01/03/2029;19/11/2026;0,05;0,06;20320,65;20305,65;20305,65",
                "Tesouro Selic;01/03/2028;sem data;0,05;0,06;20320,65;20323,10;20323,10",
                "Tesouro Selic;01/03/2027;19/11/2026;0,05;0,06;20320,65;abc;20332,84",
                "Tesouro Selic;curto",
                "Tesouro Novo;01/01/2040;19/11/2026;1;1;1;1;1",
            ]),
            TODAY,
        )
        .unwrap();
        assert_eq!(parsed.prices.len(), 1);
        assert_eq!(parsed.unreadable_rows, 3);
        assert_eq!(
            parsed.unknown_titles.into_iter().collect::<Vec<_>>(),
            ["Tesouro Novo"]
        );
    }

    #[test]
    fn a_file_without_prices_is_an_error_not_an_empty_catalog() {
        for text in [
            "",
            "\n\n",
            "<html>erro</html>",
            HEADER,
            &csv(&["Tesouro Selic;01/03/2020;19/11/2019;0,05;0,06;1,00;1,00;1,00"]),
        ] {
            assert!(
                matches!(parse_csv(text, TODAY), Err(FeedError::Malformed(_))),
                "{text:?}"
            );
        }
        // Cabeçalho sem a coluna do preço de resgate.
        let error = parse_csv("Tipo Titulo;Data Vencimento;Data Base\nx;y;z", TODAY).unwrap_err();
        assert!(error.to_string().contains("pu venda manha"), "{error}");
    }

    #[test]
    fn dates_and_prices_follow_the_brazilian_format() {
        assert_eq!(parse_date("19/11/2026"), Some(date!(2026 - 11 - 19)));
        assert_eq!(parse_date(" 01/03/2027 "), Some(date!(2027 - 03 - 01)));
        for bad in [
            "",
            "2026-11-19",
            "31/02/2026",
            "19/13/2026",
            "19/11",
            "1/2/3/4",
        ] {
            assert_eq!(parse_date(bad), None, "{bad:?}");
        }
        assert_eq!(parse_price("20332,84"), Some(dec!(20332.84)));
        assert_eq!(parse_price(" 0,00 "), Some(Decimal::ZERO));
        assert_eq!(parse_price("abc"), None);
        assert_eq!(parse_price(""), None);
    }

    #[test]
    fn the_base_date_is_midnight_in_brasilia_and_today_follows_brasilia() {
        assert_eq!(
            base_instant(date!(2026 - 11 - 19)),
            datetime!(2026-11-19 03:00 UTC)
        );
        // 01h UTC ainda é a noite anterior em Brasília.
        assert_eq!(
            today_in_brasilia(datetime!(2026-11-26 01:00 UTC)),
            date!(2026 - 11 - 25)
        );
        assert_eq!(
            today_in_brasilia(datetime!(2026-11-26 03:00 UTC)),
            date!(2026 - 11 - 26)
        );
    }
}
