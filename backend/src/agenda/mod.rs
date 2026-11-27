//! Agenda: a tarefa que mantém em dia os documentos das companhias e dos FIIs
//! da carteira, e a leitura que junta tudo na linha do tempo da pessoa. A
//! regra está em `domain::agenda`; o porquê de cada escolha, em
//! `docs/agenda.md`.
//!
//! **Fontes.** Os dados abertos da CVM (`dados.cvm.gov.br`): o IPE de cada ano
//! (fato relevante, comunicado, aviso, assembleia) e o índice do ITR e da DFP
//! (a data de entrega do resultado), ver [`cvm`]. Para FII, o Fundos.NET da B3,
//! ver [`fundos_net`]. Proventos, cotas e o calendário de pregões já estão no
//! banco e no domínio.
//!
//! **Uma vez por dia, sem baixar à toa.** Cada arquivo da CVM é conferido com
//! GET condicional, no mesmo esquema dos fundamentos (`cvm_sources`, reserva
//! por réplica, transação curta com `pg_advisory_xact_lock`). Do ITR e da DFP
//! só a primeira entrada do `.zip` é lida, e o download para ali. Cada fundo é
//! buscado no Fundos.NET uma vez por dia. Só as companhias e os fundos de
//! alguma carteira entram; fonte fora do ar deixa o gravado como está.

pub mod cvm;
pub mod fundos_net;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use self::cvm::{filing_path, ipe_path, parse_ipe, parse_receipts, FilingReceipt};
use self::fundos_net::FundDocuments;
use crate::domain::agenda::unusual::{
    by_ex_date, merged, ratio_for, unusual, Declaration, History, Unusual,
};
use crate::domain::agenda::{
    build, mark_for, undated_income, undated_rows, Declared, DeclaredRow, Document,
    DocumentCategory, DocumentSource, Event, IncomeItem, Input,
};
use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::fundamentals::{coverage, Coverage, FilingKind};
use crate::domain::income::estimate::{b3_unit_payments, yahoo_unit_payments};
use crate::domain::income::{today_in_brazil, DividendPoint, IncomeKind};
use crate::domain::income_auto::{cash_kind, ex_date_after, CashAnnouncement};
use crate::domain::tax::calendar::{covered_years, trading_closures};
use crate::fundamentals::download::{self, FetchError, Opened, Validators};
use crate::fundamentals::{due_with, Mode, CLAIM_FOR};
use crate::store::agenda::{AgendaFreshness, DocumentOwner, FileVersion};
use crate::store::fundamentals::SourceState;
use crate::store::{Asset, Store, StoreError};

/// De quanto em quanto tempo a tarefa confere o que venceu.
pub const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// Cada fundo é buscado no Fundos.NET uma vez por dia.
pub const FUND_TTL: time::Duration = time::Duration::hours(20);
/// Depois de uma falha no Fundos.NET, espera isto antes de tentar o mesmo fundo.
pub const FUND_RETRY: time::Duration = time::Duration::hours(6);
/// Pausa entre duas requisições ao Fundos.NET.
pub const PAUSE: Duration = Duration::from_millis(800);
/// Quanto a Agenda guarda para trás: dois anos e pouco (a tela mostra até 400
/// dias de uma vez).
pub const KEEP_DAYS: i64 = 800;
/// De quanto histórico de proventos a conta do fora do padrão precisa: os 5
/// anos antes do anúncio mais velho que a B3 ainda mostra.
const HISTORY_DAYS: i64 = 7 * 365;

const CVM_BASE: &str = "https://dados.cvm.gov.br";

/// Um arquivo da CVM que a Agenda lê.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CvmFile {
    /// Os documentos eventuais entregues no ano (IPE).
    Documents(i32),
    /// O índice do ITR ou da DFP do ano: a data de entrega de cada um.
    Receipts(FilingKind, i32),
}

impl CvmFile {
    /// A chave em `cvm_sources`.
    pub fn key(self) -> String {
        match self {
            CvmFile::Documents(year) => format!("documents:{year}"),
            CvmFile::Receipts(kind, year) => format!("receipts_{}:{year}", kind.as_str()),
        }
    }

    pub fn path(self) -> String {
        match self {
            CvmFile::Documents(year) => ipe_path(year),
            CvmFile::Receipts(kind, year) => filing_path(kind, year),
        }
    }
}

/// Os arquivos de uma rodada: o ano de hoje e o anterior (a CVM atualiza os
/// dois toda semana).
pub fn files(year: i32) -> Vec<CvmFile> {
    let mut list = Vec::new();
    for y in [year, year - 1] {
        list.push(CvmFile::Documents(y));
        list.push(CvmFile::Receipts(FilingKind::Quarterly, y));
        list.push(CvmFile::Receipts(FilingKind::Annual, y));
    }
    list
}

/// O que uma rodada fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoundReport {
    /// Arquivos da CVM lidos e gravados.
    pub updated: usize,
    /// Arquivos que a CVM disse não terem mudado (304) ou que ainda não existem.
    pub unchanged: usize,
    pub failed: usize,
    pub funds_fetched: usize,
    pub funds_failed: usize,
}

pub struct AgendaService {
    store: Store,
    /// `None`: fontes externas desligadas; a Agenda mostra o que já está gravado.
    http: Option<reqwest::Client>,
    funds: Option<Arc<dyn FundDocuments>>,
    base: String,
    pause: Duration,
}

impl AgendaService {
    pub fn new(store: Store, http: reqwest::Client, funds: Arc<dyn FundDocuments>) -> Self {
        Self {
            store,
            http: Some(http),
            funds: Some(funds),
            base: CVM_BASE.to_owned(),
            pause: PAUSE,
        }
    }

    /// Sem fonte: nada é buscado.
    pub fn disabled(store: Store) -> Self {
        Self {
            store,
            http: None,
            funds: None,
            base: CVM_BASE.to_owned(),
            pause: PAUSE,
        }
    }

    /// Para os testes apontarem a CVM para um servidor falso.
    pub fn with_base(mut self, base: &str) -> Self {
        base.trim_end_matches('/').clone_into(&mut self.base);
        self
    }

    /// Pausa entre requisições ao Fundos.NET (os testes usam zero).
    pub fn with_pause(mut self, pause: Duration) -> Self {
        self.pause = pause;
        self
    }

    pub fn enabled(&self) -> bool {
        self.http.is_some()
    }

    /// Uma rodada: os arquivos da CVM vencidos e os fundos vencidos.
    pub async fn round(&self, now: OffsetDateTime) -> RoundReport {
        let mut report = RoundReport::default();
        let Some(http) = &self.http else {
            return report;
        };
        let wanted = match self.store.fundamentals_wanted().await {
            Ok(wanted) => wanted,
            Err(error) => {
                tracing::error!(error = %error, "carteiras ilegiveis para a agenda");
                return report;
            }
        };
        let issuers: Vec<String> = wanted.companies.iter().cloned().collect();
        let codes: BTreeSet<i32> = match self.store.issuer_codes(&issuers).await {
            Ok(known) => known.values().filter_map(|c| c.cvm_code).collect(),
            Err(error) => {
                tracing::error!(error = %error, "codigos CVM ilegiveis para a agenda");
                BTreeSet::new()
            }
        };
        if !codes.is_empty() {
            self.cvm_files(http, &codes, now, &mut report).await;
        }
        let funds: Vec<String> = wanted.funds.iter().cloned().collect();
        self.fund_documents(&funds, now, &mut report).await;
        let before = today_in_brazil(now).saturating_sub(time::Duration::days(KEEP_DAYS));
        if let Err(error) = self.store.prune_corporate_documents(before).await {
            tracing::error!(error = %error, "documentos velhos nao apagados");
        }
        report
    }

    async fn cvm_files(
        &self,
        http: &reqwest::Client,
        codes: &BTreeSet<i32>,
        now: OffsetDateTime,
        report: &mut RoundReport,
    ) {
        let states = match self.store.cvm_source_states().await {
            Ok(states) => states,
            Err(error) => {
                tracing::error!(error = %error, "estado dos arquivos da CVM ilegivel");
                return;
            }
        };
        for file in files(today_in_brazil(now).year()) {
            let key = file.key();
            if due_with(true, states.get(&key), codes, now).is_none() {
                continue;
            }
            let fresh = match self.store.claim_cvm_source(&key, now, CLAIM_FOR).await {
                Ok(Some(fresh)) => fresh,
                Ok(None) => continue,
                Err(error) => {
                    tracing::error!(source = %key, error = %error, "reserva do arquivo falhou");
                    continue;
                }
            };
            // A decisão que vale é a tomada com o estado lido ao reservar.
            let Some(mode) = due_with(true, Some(&fresh), codes, now) else {
                if let Err(error) = self.store.release_cvm_source(&key).await {
                    tracing::error!(source = %key, error = %error, "reserva nao solta");
                }
                continue;
            };
            match self.read(http, file, &mode, &fresh, codes, now).await {
                Ok(true) => {
                    report.updated += 1;
                    tracing::info!(source = %key, "arquivo da CVM da agenda lido");
                }
                Ok(false) => report.unchanged += 1,
                Err(reason) => {
                    report.failed += 1;
                    tracing::warn!(source = %key, reason = %reason, "arquivo da CVM da agenda nao lido");
                    if let Err(error) = self
                        .store
                        .record_cvm_source_failure(&key, now, &reason)
                        .await
                    {
                        tracing::error!(source = %key, error = %error, "falha nao registrada");
                    }
                }
            }
        }
    }

    /// Lê um arquivo e grava. `Ok(false)`: nada mudou (304, ou o arquivo do
    /// ano ainda não existe).
    async fn read(
        &self,
        http: &reqwest::Client,
        file: CvmFile,
        mode: &Mode,
        state: &SourceState,
        wanted: &BTreeSet<i32>,
        now: OffsetDateTime,
    ) -> Result<bool, String> {
        let key = file.key();
        let url = format!("{}{}", self.base, file.path());
        let known = match mode {
            Mode::Revalidate => Some(&state.validators).filter(|v| !v.is_empty()),
            Mode::Full | Mode::Extend(_) => None,
        };
        let download = match download::open(http, &url, known).await {
            Ok(Opened::NotModified) => {
                self.store
                    .record_cvm_source_unchanged(&key, now)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(false);
            }
            Ok(Opened::Body(download)) => download,
            Err(FetchError::NotFound) => {
                self.store
                    .record_cvm_source_absent(&key, wanted, now)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(false);
            }
            Err(error) => return Err(error.to_string()),
        };
        let validators: Validators = download.validators.clone();
        // A mesma versão já lida, com companhias novas: lê só as novas.
        let same_version = state.checked_at.is_some() && state.validators.same_version(&validators);
        let (read, scanned): (BTreeSet<i32>, BTreeSet<i32>) = match mode {
            Mode::Extend(missing) if same_version => (
                missing.clone(),
                state.scanned.union(missing).copied().collect(),
            ),
            _ => (wanted.clone(), wanted.clone()),
        };
        let codes: HashSet<i32> = read.iter().copied().collect();
        let version = FileVersion {
            key: &key,
            validators: &validators,
            scanned: &scanned,
            now,
        };
        let saved = match file {
            CvmFile::Documents(year) => {
                let documents = download
                    .parse(move |reader| parse_ipe(reader, year, &codes))
                    .await
                    .map_err(|e| e.to_string())?;
                self.store
                    .save_company_documents(version, year, &documents, &read)
                    .await
            }
            CvmFile::Receipts(kind, year) => {
                let receipts = download
                    .parse(move |reader| parse_receipts(reader, kind, year, &codes))
                    .await
                    .map_err(|e| e.to_string())?;
                self.store.save_filing_receipts(version, &receipts).await
            }
        };
        saved.map(|_| true).map_err(|e| e.to_string())
    }

    /// Os documentos de cada FII da carteira cuja busca venceu.
    async fn fund_documents(
        &self,
        issuers: &[String],
        now: OffsetDateTime,
        report: &mut RoundReport,
    ) {
        let Some(funds) = &self.funds else {
            return;
        };
        let cnpjs = match self.store.agenda_fund_cnpjs(issuers).await {
            Ok(cnpjs) => cnpjs,
            Err(error) => {
                tracing::error!(error = %error, "CNPJ dos fundos ilegivel");
                return;
            }
        };
        let mut list: Vec<String> = cnpjs.into_values().collect();
        list.sort();
        list.dedup();
        let mut first = true;
        for cnpj in list {
            match self
                .store
                .claim_fund_documents(&cnpj, now, FUND_TTL, FUND_RETRY, CLAIM_FOR)
                .await
            {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::error!(error = %error, "reserva do fundo falhou");
                    continue;
                }
            }
            if !first {
                tokio::time::sleep(self.pause).await;
            }
            first = false;
            let outcome = match funds.documents(&cnpj).await {
                Ok(documents) => self
                    .store
                    .save_fund_documents(&cnpj, &documents, now)
                    .await
                    .map_err(|e| e.to_string()),
                Err(reason) => Err(reason),
            };
            match outcome {
                Ok(_) => report.funds_fetched += 1,
                Err(reason) => {
                    report.funds_failed += 1;
                    tracing::warn!(reason = %reason, "documentos do fundo indisponiveis");
                    if let Err(error) = self
                        .store
                        .record_fund_documents_failure(&cnpj, now, &reason)
                        .await
                    {
                        tracing::error!(error = %error, "falha do fundo nao registrada");
                    }
                }
            }
        }
    }

    /// A rodada periódica, até o processo encerrar: a primeira na hora.
    pub fn spawn(self: Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let report = self.round(OffsetDateTime::now_utc()).await;
                tracing::info!(
                    updated = report.updated,
                    unchanged = report.unchanged,
                    failed = report.failed,
                    funds_fetched = report.funds_fetched,
                    funds_failed = report.funds_failed,
                    "agenda conferida"
                );
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------------

/// A Agenda de uma pessoa num período.
#[derive(Debug, Clone)]
pub struct Agenda {
    pub today: Date,
    pub from: Date,
    pub to: Date,
    pub events: Vec<Event>,
    /// Os ativos que os eventos citam.
    pub assets: HashMap<Uuid, Asset>,
    /// O último ano com calendário de pregões cadastrado.
    pub calendar_until: Option<i32>,
    pub freshness: AgendaFreshness,
}

/// Uma data com a receber fora do padrão (o selo da Renda): uma por ativo e
/// data ex, com os proventos da Renda dela (um por parcela).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusualIncome {
    pub income_ids: Vec<Uuid>,
    /// Em quantas parcelas a data com paga.
    pub installments: usize,
    pub asset_id: Uuid,
    pub record_date: Date,
    pub ex_date: Date,
    pub unusual: Unusual,
}

/// As ações e FIIs da B3: a B3 anuncia os proventos deles (a mesma regra da
/// estimativa da Renda).
fn b3_sourced(asset: &Asset) -> bool {
    asset.market() == Market::B3
        && matches!(
            asset.asset_class,
            AssetClass::BrStocks | AssetClass::BrReits
        )
}

/// Os proventos da Renda da pessoa, só dos ativos que ela tem cadastrados.
async fn income_items(
    store: &Store,
    user: Uuid,
    assets: &HashMap<Uuid, Asset>,
) -> Result<Vec<IncomeItem>, StoreError> {
    Ok(store
        .income_ledger(user)
        .await?
        .into_iter()
        .filter_map(|e| {
            let asset_id = e.asset_id.filter(|id| assets.contains_key(id))?;
            Some(IncomeItem {
                id: e.id,
                asset_id,
                kind: e.kind,
                ex_date: e.ex_date,
                pay_date: e.pay_date,
                amount_per_unit: e.amount_per_unit,
                quantity: e.quantity,
                gross: e.gross,
                net: e.net,
                source: e.source,
                external_ref: e.external_ref,
            })
        })
        .collect())
}

/// O que a B3 anunciou para cada ação e FII da pessoa, agrupado por data com,
/// com o fora do padrão de cada data com.
async fn declared_income(
    store: &Store,
    assets: &HashMap<Uuid, Asset>,
    today: Date,
) -> Result<Vec<Declared>, StoreError> {
    let candidates: Vec<(Uuid, AssetClass, String)> = assets
        .values()
        .filter(|a| b3_sourced(a))
        .filter_map(|a| Some((a.id, a.asset_class, a.symbol.clone()?)))
        .collect();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let since = today.saturating_sub(time::Duration::days(HISTORY_DAYS));
    let b3 = store.b3_income_sources(&candidates, since).await?;
    let symbols: Vec<String> = candidates
        .iter()
        .filter_map(|(_, _, symbol)| Market::B3.quote_symbol(symbol))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let cache = store.dividend_cache(&symbols).await?;
    let mut declared = Vec::new();
    for (id, class, symbol) in &candidates {
        let Some(source) = b3.get(id) else {
            continue;
        };
        // O histórico do Yahoo (em reais, bruto): o de antes do que a B3 mostra.
        let yahoo: Vec<_> = Market::B3
            .quote_symbol(symbol)
            .and_then(|s| cache.get(&s))
            .filter(|c| c.currency == "BRL")
            .map(|c| {
                let points: Vec<DividendPoint> = c
                    .events
                    .iter()
                    .map(|e| DividendPoint {
                        ex_date: e.ex_date,
                        amount: e.amount,
                    })
                    .collect();
                by_ex_date(&yahoo_unit_payments(&points, Decimal::ONE, Decimal::ZERO))
            })
            .unwrap_or_default();
        // Amortização devolve capital: fica fora do padrão de renda.
        let paid: Vec<CashAnnouncement> = source
            .cash
            .iter()
            .filter(|c| cash_kind(&c.label, *class) != IncomeKind::Amortization)
            .cloned()
            .collect();
        let per_unit = |rows: &[CashAnnouncement]| -> Decimal {
            b3_unit_payments(rows, &source.stock, *class, today)
                .iter()
                .map(|p| p.gross)
                .sum()
        };
        let mut by_kind: Vec<(IncomeKind, Vec<CashAnnouncement>)> = Vec::new();
        for cash in &paid {
            let kind = cash_kind(&cash.label, *class);
            match by_kind.iter_mut().find(|(k, _)| *k == kind) {
                Some((_, rows)) => rows.push(cash.clone()),
                None => by_kind.push((kind, vec![cash.clone()])),
            }
        }
        let history = History {
            by_kind: by_kind
                .iter()
                .map(|(kind, rows)| {
                    let payments = b3_unit_payments(rows, &source.stock, *class, today);
                    (*kind, by_ex_date(&payments))
                })
                .collect(),
            totals: merged(
                &by_ex_date(&b3_unit_payments(&paid, &source.stock, *class, today)),
                &yahoo,
            ),
        };
        let mut groups: HashMap<Date, Vec<&CashAnnouncement>> = HashMap::new();
        for cash in &source.cash {
            groups.entry(cash.record_date).or_default().push(cash);
        }
        for (record_date, rows) in groups {
            let ex_date = ex_date_after(record_date);
            let declaration = Declaration {
                ex_date,
                by_kind: by_kind
                    .iter()
                    .map(|(kind, all)| {
                        let mine: Vec<CashAnnouncement> = all
                            .iter()
                            .filter(|c| c.record_date == record_date)
                            .cloned()
                            .collect();
                        (*kind, per_unit(&mine))
                    })
                    .filter(|(_, value)| *value > Decimal::ZERO)
                    .collect(),
                total: per_unit(
                    &paid
                        .iter()
                        .filter(|c| c.record_date == record_date)
                        .cloned()
                        .collect::<Vec<_>>(),
                ),
            };
            declared.push(Declared {
                asset_id: *id,
                record_date,
                ex_date,
                rows: rows
                    .iter()
                    .map(|r| DeclaredRow {
                        label: r.label.clone(),
                        rate: r.rate,
                        pay_date: r.pay_date,
                        approved_on: r.approved_on,
                    })
                    .collect(),
                unusual: unusual(&declaration, &history, ratio_for(*class)),
            });
        }
    }
    declared.sort_by_key(|d| (d.asset_id, d.record_date));
    Ok(declared)
}

/// Quem cada documento cita: companhia pelo código CVM, FII pelo CNPJ.
struct Owners {
    companies: HashMap<i32, Vec<Uuid>>,
    funds: HashMap<String, Vec<Uuid>>,
}

async fn owners(store: &Store, assets: &HashMap<Uuid, Asset>) -> Result<Owners, StoreError> {
    let mut company_assets: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut fund_assets: HashMap<String, Vec<Uuid>> = HashMap::new();
    for asset in assets.values() {
        match coverage(asset.asset_class, asset.market(), asset.symbol.as_deref()) {
            Coverage::Company { issuer } => {
                company_assets.entry(issuer).or_default().push(asset.id)
            }
            Coverage::Fund { issuer } => fund_assets.entry(issuer).or_default().push(asset.id),
            Coverage::Unsupported(_) => {}
        }
    }
    let issuers: Vec<String> = company_assets.keys().cloned().collect();
    let mut companies: HashMap<i32, Vec<Uuid>> = HashMap::new();
    for (issuer, code) in store.issuer_codes(&issuers).await? {
        if let (Some(code), Some(ids)) = (code.cvm_code, company_assets.get(&issuer)) {
            companies.entry(code).or_default().extend(ids);
        }
    }
    let fund_issuers: Vec<String> = fund_assets.keys().cloned().collect();
    let mut funds: HashMap<String, Vec<Uuid>> = HashMap::new();
    for (issuer, cnpj) in store.agenda_fund_cnpjs(&fund_issuers).await? {
        if let Some(ids) = fund_assets.get(&issuer) {
            funds.entry(cnpj).or_default().extend(ids);
        }
    }
    for ids in companies.values_mut().chain(funds.values_mut()) {
        ids.sort_unstable();
        ids.dedup();
    }
    Ok(Owners { companies, funds })
}

fn receipt_document(receipt: &FilingReceipt) -> Document {
    Document {
        source: DocumentSource::CvmFiling,
        category: DocumentCategory::Results,
        title: None,
        subject: None,
        reference_date: Some(receipt.reference_date),
        delivered_on: receipt.received_on,
        version: receipt.version,
        url: receipt.url.clone(),
        filing: Some(receipt.kind),
    }
}

/// A Agenda da pessoa em `[from, to]`, só com o que está no banco.
pub async fn agenda(
    store: &Store,
    user: Uuid,
    today: Date,
    from: Date,
    to: Date,
) -> Result<Agenda, StoreError> {
    let assets: HashMap<Uuid, Asset> = store
        .assets(user)
        .await?
        .into_iter()
        .map(|a| (a.id, a))
        .collect();
    let income = income_items(store, user, &assets).await?;
    let declared = declared_income(store, &assets, today).await?;

    let mut undated = Vec::new();
    let pending: Vec<(&Declared, Vec<&DeclaredRow>)> = declared
        .iter()
        .map(|d| (d, undated_rows(d, &income)))
        .filter(|(_, rows)| !rows.is_empty())
        .collect();
    if !pending.is_empty() {
        let ledgers = store.entries_by_asset(user).await?;
        for (declared, rows) in pending {
            let (Some(asset), Some(entries)) = (
                assets.get(&declared.asset_id),
                ledgers.get(&declared.asset_id),
            ) else {
                continue;
            };
            for row in rows {
                match undated_income(declared, row, asset.asset_class, entries, today) {
                    Ok(Some(detail)) => undated.push((declared.asset_id, detail)),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::error!(error = %error, "livro nao fecha na agenda");
                        break;
                    }
                }
            }
        }
    }

    let owners = owners(store, &assets).await?;
    let codes: Vec<i32> = owners.companies.keys().copied().collect();
    let cnpjs: Vec<String> = owners.funds.keys().cloned().collect();
    let mut documents: Vec<(Vec<Uuid>, Document)> = store
        .agenda_documents(&codes, &cnpjs, from, to)
        .await?
        .into_iter()
        .map(|(owner, document)| {
            let ids = match &owner {
                DocumentOwner::Company(code) => owners.companies.get(code),
                DocumentOwner::Fund(cnpj) => owners.funds.get(cnpj),
            };
            (ids.cloned().unwrap_or_default(), document)
        })
        .collect();
    documents.extend(
        store
            .filing_receipts(&codes, from, to)
            .await?
            .iter()
            .map(|r| {
                (
                    owners
                        .companies
                        .get(&r.cvm_code)
                        .cloned()
                        .unwrap_or_default(),
                    receipt_document(r),
                )
            }),
    );
    let freshness = store.agenda_freshness(&cnpjs).await?;

    let events = build(&Input {
        today,
        from,
        to,
        income,
        declared,
        undated,
        documents,
        closures: trading_closures(from, to),
    });
    Ok(Agenda {
        today,
        from,
        to,
        events,
        assets,
        calendar_until: covered_years().into_iter().max(),
        freshness,
    })
}

/// Os proventos a receber da pessoa que estão fora do padrão.
pub async fn unusual_income(
    store: &Store,
    user: Uuid,
    today: Date,
) -> Result<Vec<UnusualIncome>, StoreError> {
    let assets: HashMap<Uuid, Asset> = store
        .assets(user)
        .await?
        .into_iter()
        .map(|a| (a.id, a))
        .collect();
    let income = income_items(store, user, &assets).await?;
    let declared = declared_income(store, &assets, today).await?;
    // Uma por data com: as parcelas a receber da mesma data com são um item só.
    let mut list: Vec<UnusualIncome> = Vec::new();
    for item in income.iter().filter(|item| item.pay_date > today) {
        let Some(found) = mark_for(item, &declared) else {
            continue;
        };
        let Some(mark) = found.unusual else {
            continue;
        };
        match list
            .iter_mut()
            .find(|u| u.asset_id == found.asset_id && u.ex_date == found.ex_date)
        {
            Some(existing) => existing.income_ids.push(item.id),
            None => list.push(UnusualIncome {
                income_ids: vec![item.id],
                installments: found.installments(),
                asset_id: found.asset_id,
                record_date: found.record_date,
                ex_date: found.ex_date,
                unusual: mark,
            }),
        }
    }
    for entry in &mut list {
        entry.income_ids.sort_unstable();
    }
    list.sort_by_key(|u| (u.record_date, u.asset_id));
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_of_a_round() {
        let all = files(2026);
        assert_eq!(all.len(), 6);
        assert_eq!(CvmFile::Documents(2026).key(), "documents:2026");
        assert_eq!(
            CvmFile::Receipts(FilingKind::Quarterly, 2025).key(),
            "receipts_quarterly:2025"
        );
        assert_eq!(
            CvmFile::Receipts(FilingKind::Annual, 2025).path(),
            "/dados/CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_2025.zip"
        );
        assert_eq!(
            CvmFile::Documents(2025).path(),
            "/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_2025.zip"
        );
        // A chave cabe na regra de `cvm_sources` (`^[a-z_]+(:[0-9]{4})?$`).
        for file in all {
            let key = file.key();
            let (name, year) = key.split_once(':').unwrap_or_default();
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{key}"
            );
            assert_eq!(year.len(), 4, "{key}");
        }
    }
}
