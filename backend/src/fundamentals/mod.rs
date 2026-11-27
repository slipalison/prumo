//! Fundamentos: a tarefa que mantém em dia os dados da CVM das empresas e dos
//! FIIs da carteira. A conta está em `domain::fundamentals`; o porquê de cada
//! escolha, em `docs/fundamentos.md`.
//!
//! **Fontes.** Os dados abertos da CVM (`dados.cvm.gov.br`, ver [`cvm`]) e,
//! para ligar o ticker à companhia, o código CVM da página pública do emissor
//! e o número de ações, da página pública do emissor na B3 (ver [`b3`]).
//!
//! **Uma vez por dia, sem baixar à toa.** Cada arquivo é conferido com GET
//! condicional (a CVM responde 304 quando nada mudou). Só um arquivo novo, ou
//! uma companhia que entrou na carteira e ainda não foi lida naquela versão,
//! faz baixar de novo. Os arquivos de companhia são lidos em fluxo e só as
//! companhias da carteira (de todas as pessoas) ficam.
//!
//! **Duas réplicas, quatro conexões.** Cada arquivo e cada emissor é
//! *reservado* no banco (quem marca primeiro lê; a outra réplica pula) e cada
//! gravação é uma transação curta com `pg_advisory_xact_lock`. Nenhuma conexão
//! fica presa durante o download. Quem reserva decide de novo com o estado
//! devolvido pela reserva, não com a lista lida no começo da rodada.
//!
//! **Fonte fora do ar não quebra nada.** A falha fica registrada, o que já
//! estava gravado continua valendo e a próxima tentativa vem em 6 h.

pub mod b3;
pub mod cvm;
pub mod download;

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;
use std::time::Duration;

use time::OffsetDateTime;
use tokio::sync::Notify;

use self::b3::IssuerDirectory;
use self::download::{FetchError, Opened, Validators};
use crate::domain::asset_class::Market;
use crate::domain::fund_issuer::ChosenBy;
use crate::domain::fundamentals::{physical_vacancy, FilingKind, Vacancy};
use crate::domain::income::today_in_brazil;
use crate::store::fund_issuer::B3Fund;
use crate::store::fundamentals::{FundVacancy, IssuerCode, SourceState};
use crate::store::Store;

/// De quanto em quanto tempo a tarefa confere o que venceu.
pub const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// Cada arquivo é conferido uma vez por dia (a CVM regera os arquivos de
/// madrugada).
pub const REVALIDATE_AFTER: time::Duration = time::Duration::hours(20);
/// Depois de uma falha, espera isto antes de tentar o mesmo arquivo.
pub const RETRY_AFTER: time::Duration = time::Duration::hours(6);
/// A vez de uma réplica num arquivo ou emissor; passou disso, a reserva é de
/// um processo que caiu.
pub const CLAIM_FOR: time::Duration = time::Duration::minutes(30);
/// O emissor achado é conferido de novo uma vez por dia (o número de ações
/// muda com desdobramento, grupamento e recompra cancelada); "a B3 não tem
/// código" espera uma semana.
pub const CODE_FOUND_TTL: time::Duration = time::Duration::hours(20);
pub const CODE_MISSING_TTL: time::Duration = time::Duration::days(7);
/// Exercícios de DFP lidos: o ano corrente e os cinco anteriores (a janela é
/// de cinco exercícios, e o do ano corrente quase sempre ainda não saiu).
pub const ANNUAL_FILES: i32 = 6;
/// Pausa entre duas requisições à B3.
pub const PAUSE: Duration = Duration::from_millis(800);
/// Espera depois de um ativo novo, para juntar vários (importação da B3).
pub const DEBOUNCE: Duration = Duration::from_secs(30);

const CVM_BASE: &str = "https://dados.cvm.gov.br";

/// Um arquivo da CVM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Registry,
    Annual(i32),
    Quarterly(i32),
    FundMonthly(i32),
    FundQuarterly(i32),
}

impl Source {
    /// A chave em `cvm_sources`.
    pub fn key(self) -> String {
        match self {
            Source::Registry => "registry".to_owned(),
            Source::Annual(year) => format!("annual:{year}"),
            Source::Quarterly(year) => format!("quarterly:{year}"),
            Source::FundMonthly(year) => format!("fund_monthly:{year}"),
            Source::FundQuarterly(year) => format!("fund_quarterly:{year}"),
        }
    }

    pub fn path(self) -> String {
        match self {
            Source::Registry => cvm::REGISTRY_PATH.to_owned(),
            Source::Annual(year) => cvm::path(FilingKind::Annual, year),
            Source::Quarterly(year) => cvm::path(FilingKind::Quarterly, year),
            Source::FundMonthly(year) => cvm::fund_monthly_path(year),
            Source::FundQuarterly(year) => cvm::fund_quarterly_path(year),
        }
    }

    fn company_file(self) -> bool {
        matches!(self, Source::Annual(_) | Source::Quarterly(_))
    }
}

/// Como ler um arquivo agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// GET condicional: só baixa se a CVM publicou versão nova.
    Revalidate,
    /// Nunca lido: baixa e lê para todas as companhias.
    Full,
    /// A versão lida não teve estas companhias (entraram na carteira depois).
    Extend(BTreeSet<i32>),
}

/// Se o arquivo precisa ser lido agora, e como.
pub fn due(
    source: Source,
    state: Option<&SourceState>,
    wanted: &BTreeSet<i32>,
    now: OffsetDateTime,
) -> Option<Mode> {
    due_with(source.company_file(), state, wanted, now)
}

/// A mesma regra para qualquer arquivo da CVM (a Agenda lê outros):
/// `company_file` diz se o arquivo é lido só para as companhias `wanted`.
pub fn due_with(
    company_file: bool,
    state: Option<&SourceState>,
    wanted: &BTreeSet<i32>,
    now: OffsetDateTime,
) -> Option<Mode> {
    let Some(state) = state else {
        return Some(Mode::Full);
    };
    if failed_recently(state, now) {
        return None;
    }
    let Some(checked) = state.checked_at else {
        return Some(Mode::Full);
    };
    if company_file {
        let missing: BTreeSet<i32> = wanted.difference(&state.scanned).copied().collect();
        if !missing.is_empty() {
            return Some(Mode::Extend(missing));
        }
    }
    (now - checked >= REVALIDATE_AFTER).then_some(Mode::Revalidate)
}

fn failed_recently(state: &SourceState, now: OffsetDateTime) -> bool {
    state.last_error.is_some() && state.attempted_at.is_some_and(|at| now - at < RETRY_AFTER)
}

/// O modo de leitura quando o que está gravado ainda não tem tudo que este
/// código lê do arquivo (`incomplete`: o informe mensal de FII sem o ativo
/// total e o caixa, `Store::fund_balance_missing`): o arquivo inteiro, sem o
/// GET condicional (a CVM responderia 304), mesmo que ele esteja em dia. Depois
/// de uma falha, espera [`RETRY_AFTER`] como sempre.
pub fn with_backfill(
    mode: Option<Mode>,
    incomplete: bool,
    state: Option<&SourceState>,
    now: OffsetDateTime,
) -> Option<Mode> {
    if !incomplete {
        return mode;
    }
    match state {
        Some(state) if failed_recently(state, now) => None,
        _ => Some(Mode::Full),
    }
}

/// Se o emissor (código CVM e ações) precisa ser buscado na B3.
pub fn code_due(known: Option<&IssuerCode>, now: OffsetDateTime) -> bool {
    match known {
        None
        | Some(IssuerCode {
            fetched_at: None, ..
        }) => true,
        Some(IssuerCode {
            cvm_code,
            fetched_at: Some(at),
            ..
        }) => {
            let ttl = if cvm_code.is_some() {
                CODE_FOUND_TTL
            } else {
                CODE_MISSING_TTL
            };
            now - *at >= ttl
        }
    }
}

/// Se o fundo de uma sigla de FII precisa ser buscado na B3: nunca buscado,
/// achado há mais de [`CODE_FOUND_TTL`] ou não achado há mais de
/// [`CODE_MISSING_TTL`] (as mesmas idades do código CVM das companhias).
pub fn fund_due(known: Option<&B3Fund>, now: OffsetDateTime) -> bool {
    match known {
        None
        | Some(B3Fund {
            fetched_at: None, ..
        }) => true,
        Some(B3Fund {
            cnpj,
            fetched_at: Some(at),
            ..
        }) => {
            let ttl = if cnpj.is_some() {
                CODE_FOUND_TTL
            } else {
                CODE_MISSING_TTL
            };
            now - *at >= ttl
        }
    }
}

/// Os arquivos de uma rodada: os de companhia só com companhia na carteira,
/// os de FII só com FII.
pub fn sources(year: i32, companies: bool, funds: bool) -> Vec<Source> {
    let mut list = Vec::new();
    if companies {
        list.push(Source::Registry);
        list.extend((year - ANNUAL_FILES + 1..=year).rev().map(Source::Annual));
        list.extend([Source::Quarterly(year), Source::Quarterly(year - 1)]);
    }
    if funds {
        list.extend([
            Source::FundMonthly(year),
            Source::FundMonthly(year - 1),
            Source::FundQuarterly(year),
            Source::FundQuarterly(year - 1),
        ]);
    }
    list
}

/// O que uma rodada fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoundReport {
    pub codes_fetched: usize,
    pub codes_failed: usize,
    /// FIIs cujo CNPJ a B3 respondeu (achando o fundo ou não) e os que falharam.
    pub funds_fetched: usize,
    pub funds_failed: usize,
    /// Arquivos lidos (baixados) e gravados.
    pub updated: usize,
    /// Arquivos que a CVM disse não terem mudado (304) ou que ainda não existem.
    pub unchanged: usize,
    pub failed: usize,
}

pub struct FundamentalsService {
    store: Store,
    /// `None`: fontes externas desligadas; a tela mostra o que já está gravado.
    http: Option<reqwest::Client>,
    codes: Option<Arc<dyn IssuerDirectory>>,
    base: String,
    pause: Duration,
    debounce: Duration,
    wake: Notify,
}

impl FundamentalsService {
    pub fn new(store: Store, http: reqwest::Client, codes: Arc<dyn IssuerDirectory>) -> Self {
        Self {
            store,
            http: Some(http),
            codes: Some(codes),
            base: CVM_BASE.to_owned(),
            pause: PAUSE,
            debounce: DEBOUNCE,
            wake: Notify::new(),
        }
    }

    /// Sem fonte: nada é buscado, só o gravado é servido.
    pub fn disabled(store: Store) -> Self {
        Self {
            store,
            http: None,
            codes: None,
            base: CVM_BASE.to_owned(),
            pause: PAUSE,
            debounce: DEBOUNCE,
            wake: Notify::new(),
        }
    }

    /// Para os testes apontarem a CVM para um servidor falso.
    pub fn with_base(mut self, base: &str) -> Self {
        base.trim_end_matches('/').clone_into(&mut self.base);
        self
    }

    /// Pausa entre requisições à B3 e espera depois de ativo novo (os testes
    /// usam zero).
    pub fn with_timing(mut self, pause: Duration, debounce: Duration) -> Self {
        self.pause = pause;
        self.debounce = debounce;
        self
    }

    pub fn enabled(&self) -> bool {
        self.http.is_some()
    }

    /// Um ativo da B3 entrou na carteira (ou mudou de símbolo): a próxima
    /// rodada vem já, sem esperar a hora.
    pub fn prefetch(&self, market: Market, symbol: Option<&str>) {
        if self.enabled() && market == Market::B3 && symbol.is_some() {
            self.wake.notify_one();
        }
    }

    /// Busca na B3 o código CVM dos emissores que ainda não têm (ou venceram).
    async fn resolve_codes(
        &self,
        issuers: &[String],
        now: OffsetDateTime,
        report: &mut RoundReport,
    ) {
        let Some(codes) = &self.codes else {
            return;
        };
        let known = match self.store.issuer_codes(issuers).await {
            Ok(known) => known,
            Err(error) => {
                tracing::error!(error = %error, "codigos CVM ilegiveis");
                return;
            }
        };
        let mut first = true;
        for issuer in issuers {
            if !code_due(known.get(issuer), now) {
                continue;
            }
            match self.store.claim_issuer_code(issuer, now, CLAIM_FOR).await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::error!(error = %error, "reserva do emissor falhou");
                    continue;
                }
            }
            if !first {
                tokio::time::sleep(self.pause).await;
            }
            first = false;
            match codes.issuer(issuer).await {
                Ok(info) => {
                    if let Err(error) = self.store.save_issuer_code(issuer, &info, now).await {
                        tracing::error!(issuer = %issuer, error = %error, "codigo CVM nao gravado");
                        report.codes_failed += 1;
                    } else {
                        report.codes_fetched += 1;
                    }
                }
                Err(reason) => {
                    report.codes_failed += 1;
                    tracing::warn!(issuer = %issuer, reason = %reason, "codigo CVM indisponivel na B3");
                    if let Err(error) = self.store.release_issuer_code(issuer).await {
                        tracing::error!(error = %error, "reserva do emissor nao liberada");
                    }
                }
            }
        }
    }

    /// O CNPJ que a B3 lista para cada sigla de FII das carteiras (a fonte do
    /// ticker: `domain::fund_issuer`), e o aviso no log de cada emissor que
    /// mais de um fundo declara sem a B3 para decidir.
    async fn resolve_funds(&self, funds: &[String], now: OffsetDateTime, report: &mut RoundReport) {
        let Some(directory) = &self.codes else {
            return;
        };
        let known = match self.store.b3_funds(funds).await {
            Ok(known) => known,
            Err(error) => {
                tracing::error!(error = %error, "fundos da B3 ilegiveis");
                return;
            }
        };
        let mut first = true;
        for issuer in funds {
            if !fund_due(known.get(issuer), now) {
                continue;
            }
            match self.store.claim_b3_fund(issuer, now, CLAIM_FOR).await {
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
            match directory.fund(issuer).await {
                Ok(fund) => {
                    if let Err(error) = self.store.save_b3_fund(issuer, fund.as_ref(), now).await {
                        tracing::error!(issuer = %issuer, error = %error, "fundo da B3 nao gravado");
                        report.funds_failed += 1;
                    } else {
                        report.funds_fetched += 1;
                    }
                }
                Err(reason) => {
                    report.funds_failed += 1;
                    tracing::warn!(issuer = %issuer, reason = %reason, "fundo indisponivel na B3");
                    if let Err(error) = self.store.release_b3_fund(issuer).await {
                        tracing::error!(error = %error, "reserva do fundo nao liberada");
                    }
                }
            }
        }
        match self.store.fund_cnpjs(funds).await {
            Ok(chosen) => {
                for (issuer, choice) in chosen {
                    if !choice.rivals.is_empty() && choice.by != ChosenBy::B3 {
                        tracing::warn!(
                            issuer = %issuer,
                            chosen = %choice.cnpj,
                            rivals = ?choice.rivals,
                            "mais de um fundo declara o ISIN da cota; vale o que declara ha mais tempo"
                        );
                    }
                }
            }
            Err(error) => tracing::error!(error = %error, "fundos dos emissores ilegiveis"),
        }
    }

    /// Uma rodada: os códigos que faltam e os arquivos vencidos.
    pub async fn round(&self, now: OffsetDateTime) -> RoundReport {
        let mut report = RoundReport::default();
        let Some(http) = &self.http else {
            return report;
        };
        let wanted = match self.store.fundamentals_wanted().await {
            Ok(wanted) => wanted,
            Err(error) => {
                tracing::error!(error = %error, "carteiras ilegiveis para os fundamentos");
                return report;
            }
        };
        let issuers: Vec<String> = wanted.companies.iter().cloned().collect();
        self.resolve_codes(&issuers, now, &mut report).await;
        let funds: Vec<String> = wanted.funds.iter().cloned().collect();
        self.resolve_funds(&funds, now, &mut report).await;
        let codes: BTreeSet<i32> = match self.store.issuer_codes(&issuers).await {
            Ok(known) => known.values().filter_map(|c| c.cvm_code).collect(),
            Err(error) => {
                tracing::error!(error = %error, "codigos CVM ilegiveis");
                return report;
            }
        };
        let states = match self.store.cvm_source_states().await {
            Ok(states) => states,
            Err(error) => {
                tracing::error!(error = %error, "estado dos arquivos da CVM ilegivel");
                return report;
            }
        };
        let year = today_in_brazil(now).year();
        for source in sources(year, !codes.is_empty(), !wanted.funds.is_empty()) {
            let key = source.key();
            let incomplete = self.incomplete(source).await;
            if with_backfill(
                due(source, states.get(&key), &codes, now),
                incomplete,
                states.get(&key),
                now,
            )
            .is_none()
            {
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
            // A lista lida no começo da rodada pode estar velha: outra réplica
            // pode ter lido este arquivo e soltado a reserva nesse meio-tempo.
            // A decisão que vale é a tomada com o estado lido ao reservar.
            let state = Some(&fresh);
            let Some(mode) = with_backfill(due(source, state, &codes, now), incomplete, state, now)
            else {
                if let Err(error) = self.store.release_cvm_source(&key).await {
                    tracing::error!(source = %key, error = %error, "reserva nao solta");
                }
                continue;
            };
            match self.read(http, source, &mode, state, &codes, now).await {
                Ok(true) => {
                    report.updated += 1;
                    tracing::info!(source = %key, "arquivo da CVM lido");
                }
                Ok(false) => report.unchanged += 1,
                Err(reason) => {
                    report.failed += 1;
                    tracing::warn!(source = %key, reason = %reason, "arquivo da CVM nao lido");
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
        report
    }

    /// Se o gravado a partir de `source` ainda não tem tudo que este código lê
    /// dele: só o informe mensal de FII, enquanto houver fundo do ano sem o
    /// ativo total e o caixa ([`with_backfill`]).
    async fn incomplete(&self, source: Source) -> bool {
        let Source::FundMonthly(year) = source else {
            return false;
        };
        match self.store.fund_balance_missing(year).await {
            Ok(missing) => missing,
            Err(error) => {
                tracing::error!(error = %error, "caixa dos fundos ilegivel");
                false
            }
        }
    }

    /// Lê um arquivo e grava. `Ok(false)`: nada mudou (304, ou o arquivo do
    /// ano ainda não existe).
    async fn read(
        &self,
        http: &reqwest::Client,
        source: Source,
        mode: &Mode,
        state: Option<&SourceState>,
        wanted: &BTreeSet<i32>,
        now: OffsetDateTime,
    ) -> Result<bool, String> {
        let key = source.key();
        let url = format!("{}{}", self.base, source.path());
        let known = match mode {
            Mode::Revalidate => state.map(|s| &s.validators).filter(|v| !v.is_empty()),
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
        let store = &self.store;
        let saved = match source {
            Source::Annual(year) | Source::Quarterly(year) => {
                let kind = if matches!(source, Source::Annual(_)) {
                    FilingKind::Annual
                } else {
                    FilingKind::Quarterly
                };
                // A mesma versão já lida, com companhias novas: lê só as novas.
                let same_version = state.is_some_and(|s| {
                    s.checked_at.is_some() && s.validators.same_version(&validators)
                });
                let (read, scanned) = match mode {
                    Mode::Extend(missing) if same_version => (
                        missing.clone(),
                        state
                            .map(|s| s.scanned.union(missing).copied().collect())
                            .unwrap_or_else(|| missing.clone()),
                    ),
                    _ => (wanted.clone(), wanted.clone()),
                };
                let codes: HashSet<i32> = read.into_iter().collect();
                let filings = download
                    .parse(move |reader| cvm::parse_company_archive(reader, kind, year, &codes))
                    .await
                    .map_err(|e| e.to_string())?;
                store
                    .save_company_filings(&key, &filings, &validators, &scanned, now)
                    .await
                    .map(|_| ())
            }
            Source::Registry => {
                let list = download
                    .parse(cvm::parse_registry)
                    .await
                    .map_err(|e| e.to_string())?;
                store.save_registry(&key, &list, &validators, now).await
            }
            Source::FundMonthly(year) => {
                let monthly = download
                    .parse(move |reader| cvm::parse_fund_monthly(reader, year))
                    .await
                    .map_err(|e| e.to_string())?;
                store
                    .save_fund_monthly(&key, &monthly.reports, &monthly.months, &validators, now)
                    .await
            }
            Source::FundQuarterly(year) => {
                let quarters = download
                    .parse(move |reader| cvm::parse_fund_quarterly(reader, year))
                    .await
                    .map_err(|e| e.to_string())?;
                let rows: Vec<FundVacancy> = quarters
                    .into_iter()
                    .map(|q| {
                        let (value, properties) = physical_vacancy(&q.properties);
                        FundVacancy {
                            cnpj: q.cnpj,
                            vacancy: Vacancy {
                                reference_date: q.reference_date,
                                value,
                                properties,
                            },
                        }
                    })
                    .collect();
                store.save_fund_vacancy(&key, &rows, &validators, now).await
            }
        };
        saved.map(|()| true).map_err(|e| e.to_string())
    }

    /// A rodada periódica, até o processo encerrar: a primeira na hora; ativo
    /// novo adianta a próxima.
    pub fn spawn(self: Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {}
                    () = self.wake.notified() => tokio::time::sleep(self.debounce).await,
                }
                let report = self.round(OffsetDateTime::now_utc()).await;
                tracing::info!(
                    codes_fetched = report.codes_fetched,
                    codes_failed = report.codes_failed,
                    updated = report.updated,
                    unchanged = report.unchanged,
                    failed = report.failed,
                    "fundamentos conferidos"
                );
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn state(
        checked: Option<time::Duration>,
        failed: Option<time::Duration>,
        scanned: &[i32],
        now: OffsetDateTime,
    ) -> SourceState {
        SourceState {
            validators: Validators::default(),
            scanned: scanned.iter().copied().collect(),
            attempted_at: failed.or(checked).map(|ago| now - ago),
            checked_at: checked.map(|ago| now - ago),
            last_error: failed.map(|_| "fora".to_owned()),
        }
    }

    fn set(codes: &[i32]) -> BTreeSet<i32> {
        codes.iter().copied().collect()
    }

    #[test]
    fn a_file_is_read_once_a_day_and_again_for_new_companies() {
        let now = datetime!(2026-11-26 12:00 UTC);
        let annual = Source::Annual(2025);
        assert_eq!(due(annual, None, &set(&[1]), now), Some(Mode::Full));
        let fresh = state(Some(time::Duration::hours(2)), None, &[1, 2], now);
        assert_eq!(due(annual, Some(&fresh), &set(&[1, 2]), now), None);
        assert_eq!(due(annual, Some(&fresh), &set(&[1]), now), None);
        // Companhia nova: lê de novo só ela.
        assert_eq!(
            due(annual, Some(&fresh), &set(&[1, 3]), now),
            Some(Mode::Extend(set(&[3])))
        );
        // Arquivo de FII não tem lista de companhias.
        assert_eq!(
            due(Source::FundMonthly(2026), Some(&fresh), &set(&[3]), now),
            None
        );
        let old = state(Some(time::Duration::hours(21)), None, &[1], now);
        assert_eq!(
            due(annual, Some(&old), &set(&[1]), now),
            Some(Mode::Revalidate)
        );
    }

    #[test]
    fn an_incomplete_fund_report_is_read_again_in_full() {
        let now = datetime!(2026-11-26 12:00 UTC);
        let monthly = Source::FundMonthly(2026);
        let fresh = state(Some(time::Duration::hours(2)), None, &[], now);
        // Em dia: nada a ler; incompleto, o arquivo inteiro (sem 304).
        assert_eq!(due(monthly, Some(&fresh), &set(&[]), now), None);
        assert_eq!(
            with_backfill(None, true, Some(&fresh), now),
            Some(Mode::Full)
        );
        assert_eq!(
            with_backfill(Some(Mode::Revalidate), true, Some(&fresh), now),
            Some(Mode::Full)
        );
        assert_eq!(with_backfill(None, true, None, now), Some(Mode::Full));
        // Completo, vale a regra de sempre.
        assert_eq!(with_backfill(None, false, Some(&fresh), now), None);
        assert_eq!(
            with_backfill(Some(Mode::Revalidate), false, Some(&fresh), now),
            Some(Mode::Revalidate)
        );
        // Falhou há pouco: espera, mesmo incompleto.
        let failed = state(
            Some(time::Duration::days(3)),
            Some(time::Duration::hours(1)),
            &[],
            now,
        );
        assert_eq!(with_backfill(None, true, Some(&failed), now), None);
    }

    #[test]
    fn a_failure_waits_before_trying_again() {
        let now = datetime!(2026-11-26 12:00 UTC);
        let annual = Source::Annual(2025);
        let failed = state(
            Some(time::Duration::days(3)),
            Some(time::Duration::hours(1)),
            &[1],
            now,
        );
        assert_eq!(due(annual, Some(&failed), &set(&[1, 2]), now), None);
        let later = state(
            Some(time::Duration::days(3)),
            Some(time::Duration::hours(7)),
            &[1],
            now,
        );
        assert_eq!(
            due(annual, Some(&later), &set(&[1]), now),
            Some(Mode::Revalidate)
        );
        // Nunca deu certo: lê inteiro quando a espera passa.
        let never = state(None, Some(time::Duration::hours(7)), &[], now);
        assert_eq!(due(annual, Some(&never), &set(&[1]), now), Some(Mode::Full));
    }

    #[test]
    fn issuer_codes_are_kept_for_a_while() {
        let now = datetime!(2026-11-26 12:00 UTC);
        assert!(code_due(None, now));
        let found = |ago: time::Duration| IssuerCode {
            cvm_code: Some(5410),
            total_shares: None,
            fetched_at: Some(now - ago),
        };
        assert!(!code_due(Some(&found(time::Duration::hours(2))), now));
        assert!(code_due(Some(&found(time::Duration::hours(21))), now));
        let missing = |ago: time::Duration| IssuerCode {
            cvm_code: None,
            total_shares: None,
            fetched_at: Some(now - ago),
        };
        assert!(!code_due(Some(&missing(time::Duration::days(2))), now));
        assert!(code_due(Some(&missing(time::Duration::days(8))), now));
        let claimed_only = IssuerCode {
            cvm_code: None,
            total_shares: None,
            fetched_at: None,
        };
        assert!(code_due(Some(&claimed_only), now));
    }

    #[test]
    fn the_files_of_a_round() {
        let all = sources(2026, true, true);
        assert_eq!(all.len(), 1 + 6 + 2 + 4);
        assert!(all.contains(&Source::Annual(2021)));
        assert!(!all.contains(&Source::Annual(2020)));
        assert!(all.contains(&Source::Quarterly(2025)));
        assert!(sources(2026, false, false).is_empty());
        assert_eq!(sources(2026, false, true).len(), 4);
        assert_eq!(Source::Annual(2025).key(), "annual:2025");
        assert_eq!(Source::FundQuarterly(2026).key(), "fund_quarterly:2026");
        assert_eq!(
            Source::Annual(2025).path(),
            "/dados/CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_2025.zip"
        );
        assert_eq!(
            Source::FundQuarterly(2026).path(),
            "/dados/FII/DOC/INF_TRIMESTRAL/DADOS/inf_trimestral_fii_2026.zip"
        );
        assert_eq!(
            Source::Registry.path(),
            "/dados/CIA_ABERTA/CAD/DADOS/cad_cia_aberta.csv"
        );
    }
}
