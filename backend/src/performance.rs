//! Desempenho: a série da carteira (refeita a partir do livro), a tarefa
//! diária (fechamentos, foto do dia, índices), a reconstrução em segundo plano
//! quando o livro muda no passado, e o que as telas de patrimônio e
//! rentabilidade leem. A regra está em [`crate::domain::performance`] e
//! [`crate::domain::retro`]; o método e os limites, em `docs/desempenho.md`.
//! A rentabilidade por classe fica em [`classes`].
//!
//! Com duas réplicas, as duas agendam a tarefa diária no mesmo horário e as
//! duas olham as marcas de "sujo" a cada [`REBUILD_EVERY`]. Quem roda é quem
//! pega o advisory lock [`JOB_LOCK_KEY`] (`pg_try_advisory_lock`); a outra vê o
//! lock ocupado e pula. O lock é o mesmo para as duas tarefas: a diária e a
//! reconstrução nunca escrevem a série ao mesmo tempo. Mesmo que rodassem uma
//! depois da outra, a série de um trecho é sempre a mesma conta sobre o mesmo
//! livro e os caches só inserem data nova: a segunda rodada refaz o mesmo
//! resultado, sem duplicar.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use rust_decimal::Decimal;
use time::macros::offset;
use time::{Date, Duration, OffsetDateTime, Time, UtcOffset};
use uuid::Uuid;

use crate::domain::asset_class::{AssetClass, Market, UnitKind};
use crate::domain::market_rates::CDI_LOOKBACK_DAYS;
use crate::domain::monthly_stats::{self, Beat, MonthStats};
use crate::domain::net_worth::{self, Bridge, FlowPoint};
use crate::domain::performance::{
    self as rules, Accrued, MonthCell, Period, QuotaPoint, Versus, Window, YearRow,
};
use crate::domain::retro::{self, AssetHistory, Series, Start};
use crate::domain::risk;
use crate::domain::treasury::Bond;
use crate::history::{HistoryError, HistoryProvider, IndexCode};
use crate::quotes::fx_symbol;
use crate::service::{self, AssetView, PriceSource};
use crate::store::{DirtySeries, Snapshot, Store, StoreError};
use crate::treasury::history::{DisabledTreasuryHistory, TreasuryHistory};

pub mod classes;

/// Número do advisory lock da tarefa diária e da reconstrução ("PRUMO" +
/// migração 0006).
pub const JOB_LOCK_KEY: i64 = 0x50_52_55_4d_4f_06;
/// Horário de Brasília (sem horário de verão desde 2019).
pub const BRASILIA: UtcOffset = offset!(-3);
/// Os índices começam um pouco antes da primeira foto, para haver base.
const INDEX_LOOKBACK_DAYS: i64 = 10;
/// De quanto em quanto tempo a reconstrução confere as marcas de "sujo".
pub const REBUILD_EVERY: std::time::Duration = std::time::Duration::from_secs(10);
/// Quanto a marca precisa ter de idade para ser atendida: a importação da B3
/// grava centenas de lançamentos em sequência, e a série é refeita uma vez só,
/// depois que ela termina.
pub const REBUILD_SETTLE: Duration = Duration::seconds(3);

/// O dia em Brasília.
pub fn local_date(now: OffsetDateTime) -> Date {
    now.to_offset(BRASILIA).date()
}

/// A próxima vez, depois de `now`, em que o relógio de Brasília marca `at`.
pub fn next_run(now: OffsetDateTime, at: Time) -> OffsetDateTime {
    let local = now.to_offset(BRASILIA);
    let today = local.date().with_time(at).assume_offset(BRASILIA);
    if today > now {
        today
    } else {
        today + Duration::days(1)
    }
}

/// Se o horário de hoje já passou (o processo subiu depois dele).
pub fn past_todays_run(now: OffsetDateTime, at: Time) -> bool {
    now.to_offset(BRASILIA).time() >= at
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobReport {
    /// `false`: outra réplica estava com o lock.
    pub ran: bool,
    /// Pessoas com a série atualizada.
    pub snapshots: usize,
    pub failed: usize,
    pub prices: usize,
    pub indexes: usize,
    /// Objetivos que passaram a atingidos nesta rodada.
    pub objectives: usize,
    /// Fechamentos ruins da fonte apagados do cache (`domain::price_check`).
    pub bad_closes: usize,
}

/// O que uma rodada da reconstrução fez.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RebuildReport {
    /// `false`: outra réplica estava com o lock.
    pub ran: bool,
    /// Séries refeitas.
    pub users: usize,
    /// Dias gravados, somando todas.
    pub days: usize,
    pub failed: usize,
    pub prices: usize,
}

/// O que um símbolo precisa de histórico: desde quando e de quem é.
#[derive(Debug, Default)]
struct Need {
    from: Option<Date>,
    market: Option<Market>,
    holders: BTreeSet<Uuid>,
}

impl Need {
    fn add(&mut self, from: Date, market: Market, user: Uuid) {
        self.from = Some(self.from.map_or(from, |f| f.min(from)));
        self.market.get_or_insert(market);
        self.holders.insert(user);
    }
}

/// Um título do Tesouro que precisa de histórico.
struct BondNeed {
    symbol: String,
    from: Date,
    holders: BTreeSet<Uuid>,
}

pub struct Performance {
    store: Store,
    history: Arc<dyn HistoryProvider>,
    treasury: Arc<dyn TreasuryHistory>,
}

impl Performance {
    pub fn new(store: Store, history: Arc<dyn HistoryProvider>) -> Self {
        Self {
            store,
            history,
            treasury: Arc::new(DisabledTreasuryHistory),
        }
    }

    /// O histórico dos títulos do Tesouro (o CSV do Tesouro Transparente).
    pub fn with_treasury_history(mut self, treasury: Arc<dyn TreasuryHistory>) -> Self {
        self.treasury = treasury;
        self
    }

    /// A tarefa do dia: fechamentos dos ativos, a série de cada carteira até
    /// hoje e os índices. Só roda com o lock; sem ele, volta `ran: false`.
    pub async fn run_daily(&self, now: OffsetDateTime) -> Result<JobReport, StoreError> {
        let Some(lock) = self.store.try_job_lock(JOB_LOCK_KEY).await? else {
            tracing::info!("tarefa diaria de desempenho ja esta rodando em outra replica");
            return Ok(JobReport::default());
        };
        let report = self.run_locked(now).await;
        lock.release().await;
        report
    }

    async fn run_locked(&self, now: OffsetDateTime) -> Result<JobReport, StoreError> {
        let today = local_date(now);
        let mut report = JobReport {
            ran: true,
            prices: self.sync_prices(now).await?,
            ..JobReport::default()
        };
        // O cache inteiro conferido: fechamento ruim que entrou antes de a volta
        // chegar sai agora, e a série de quem o tinha é refeita logo abaixo.
        report.bad_closes = self.store.clean_bad_closes().await?;
        for user in self.store.snapshot_users().await? {
            match self.daily_series(user, today, now).await {
                Ok(true) => report.snapshots += 1,
                Ok(false) => {}
                Err(error) => {
                    report.failed += 1;
                    tracing::error!(error = %error, "serie da carteira nao gravada");
                }
            }
        }
        // Antes dos índices, que podem falhar: a data de conquista não espera
        // por eles. Falha aqui não derruba a tarefa.
        match crate::objectives::record_achieved(&self.store, now).await {
            Ok(count) => report.objectives = count,
            Err(error) => {
                report.failed += 1;
                tracing::error!(error = %error, "objetivos atingidos nao apurados");
            }
        }
        report.indexes = self.sync_indexes(today).await?;
        Ok(report)
    }

    /// A série da pessoa até hoje: do dia seguinte à última foto (a de hoje é
    /// sempre refeita) ou desde a marca de "sujo", o que vier primeiro.
    async fn daily_series(
        &self,
        user: Uuid,
        today: Date,
        now: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let dirty = self.store.dirty_since(user).await?;
        let next = match self.store.last_snapshot_date(user).await? {
            Some(last) => Some(last + Duration::days(1)),
            None => self.store.first_trade_date(user).await?,
        };
        let since = [next, dirty.map(|d| d.since)]
            .into_iter()
            .flatten()
            .min()
            .map(|d| d.min(today));
        let written = match since {
            Some(since) => rebuild_series(&self.store, user, since, today, now).await? > 0,
            None => false,
        };
        if let Some(dirty) = dirty {
            self.store.clear_dirty(&dirty).await?;
        }
        Ok(written)
    }

    /// Apaga do cache de preços os fechamentos ruins da fonte e marca a série
    /// de quem tinha o ativo. Só roda com o lock (`None`: outra réplica está
    /// com ele, e a tarefa diária confere de novo).
    pub async fn clean_prices(&self) -> Result<Option<usize>, StoreError> {
        let Some(lock) = self.store.try_job_lock(JOB_LOCK_KEY).await? else {
            return Ok(None);
        };
        let removed = self.store.clean_bad_closes().await;
        lock.release().await;
        removed.map(Some)
    }

    /// Refaz as séries marcadas como sujas há pelo menos [`REBUILD_SETTLE`],
    /// cada uma desde a própria marca até a última foto (ou ontem). Antes,
    /// baixa o preço que falta. Só roda com o lock.
    pub async fn run_pending(&self, now: OffsetDateTime) -> Result<RebuildReport, StoreError> {
        if self
            .store
            .dirty_series(now - REBUILD_SETTLE)
            .await?
            .is_empty()
        {
            return Ok(RebuildReport {
                ran: true,
                ..RebuildReport::default()
            });
        }
        let Some(lock) = self.store.try_job_lock(JOB_LOCK_KEY).await? else {
            return Ok(RebuildReport::default());
        };
        let report = self.rebuild_locked(now).await;
        lock.release().await;
        report
    }

    async fn rebuild_locked(&self, now: OffsetDateTime) -> Result<RebuildReport, StoreError> {
        let today = local_date(now);
        let mut report = RebuildReport {
            ran: true,
            // O preço antigo que chegar agora marca quem o tem; a lista de
            // sujos é lida depois, já com essas marcas.
            prices: self.sync_prices(now).await?,
            ..RebuildReport::default()
        };
        for dirty in self.store.dirty_series(now - REBUILD_SETTLE).await? {
            match self.rebuild_dirty(&dirty, today, now).await {
                Ok(days) => {
                    report.users += 1;
                    report.days += days;
                }
                Err(error) => {
                    report.failed += 1;
                    tracing::error!(error = %error, "serie da carteira nao refeita");
                }
            }
        }
        if report.users > 0 {
            // A série pode ter passado a começar mais cedo.
            self.sync_indexes(today).await?;
        }
        Ok(report)
    }

    async fn rebuild_dirty(
        &self,
        dirty: &DirtySeries,
        today: Date,
        now: OffsetDateTime,
    ) -> Result<usize, StoreError> {
        let yesterday = today - Duration::days(1);
        // A foto de hoje só existe depois do fechamento; antes dele, hoje é a
        // foto parcial que a rota calcula na hora.
        let end = self
            .store
            .last_snapshot_date(dirty.user)
            .await?
            .map_or(yesterday, |last| last.max(yesterday));
        let days = rebuild_series(&self.store, dirty.user, dirty.since, end, now).await?;
        self.store.clear_dirty(dirty).await?;
        Ok(days)
    }

    /// O histórico de preço de cada ativo com livro, desde o lançamento mais
    /// antigo dele ou desde o começo da janela mais longa do risco
    /// (`domain::risk::history_start`), o que vier antes, e o do câmbio da
    /// moeda dele. Para cada símbolo baixa só o que falta: para trás, antes do
    /// que já foi pedido; para a frente, depois do último dia guardado. Preço
    /// que chega para um dia que já passou marca como suja, dali em diante, a
    /// série de quem tem o ativo. Devolve quantos dias entraram.
    async fn sync_prices(&self, now: OffsetDateTime) -> Result<usize, StoreError> {
        let today = local_date(now);
        let risk_from = risk::history_start(today);
        let mut needs: BTreeMap<String, Need> = BTreeMap::new();
        for held in self.store.held_symbols().await? {
            if held.manual_price {
                continue;
            }
            if let Some(symbol) = held.market.quote_symbol(&held.symbol) {
                // O título que paga no caminho fica fora do risco por ativo
                // (`risk::pays_before_maturity`): não precisa do histórico longo.
                let pays = held.market == Market::Treasury
                    && Bond::from_symbol(&symbol)
                        .is_some_and(|b| risk::pays_before_maturity(b.kind));
                let from = if pays {
                    held.first_trade
                } else {
                    held.first_trade.min(risk_from)
                };
                needs
                    .entry(symbol)
                    .or_default()
                    .add(from, held.market, held.user);
            }
        }
        let listed: Vec<String> = needs.keys().cloned().collect();
        let currencies = self.store.quote_currency_of(&listed).await?;
        let mut fx: BTreeMap<String, Need> = BTreeMap::new();
        for (symbol, need) in &needs {
            let (Some(from), Some(market)) = (need.from, need.market) else {
                continue;
            };
            let currency = currencies
                .get(symbol)
                .map_or_else(|| default_currency(market).to_owned(), Clone::clone);
            if currency == "BRL" {
                continue;
            }
            let rate = fx.entry(fx_symbol(&currency)).or_default();
            for user in &need.holders {
                rate.add(from, Market::Us, *user);
            }
        }
        let mut stored = 0;
        let mut bonds: BTreeMap<Bond, BondNeed> = BTreeMap::new();
        for (symbol, need) in needs.into_iter().chain(fx) {
            let Some(from) = need.from else { continue };
            if need.market == Some(Market::Treasury) {
                // O preço de hoje é a cotação do dia; a tabela só é baixada
                // quando falta dia de antes do que já está guardado.
                let covered = self.store.price_covered_from(&symbol).await?;
                if from < today && covered.is_none_or(|c| from < c) {
                    if let Some(bond) = Bond::from_symbol(&symbol) {
                        bonds.insert(
                            bond,
                            BondNeed {
                                symbol,
                                from,
                                holders: need.holders,
                            },
                        );
                    }
                }
                continue;
            }
            stored += self.sync_symbol(&symbol, from, &need.holders, now).await?;
        }
        stored += self.sync_bonds(bonds, today).await?;
        Ok(stored)
    }

    async fn sync_symbol(
        &self,
        symbol: &str,
        from: Date,
        holders: &BTreeSet<Uuid>,
        now: OffsetDateTime,
    ) -> Result<usize, StoreError> {
        let today = local_date(now);
        let covered = self.store.price_covered_from(symbol).await?;
        let last = self.store.last_price_date(symbol).await?;
        // (de, até, para trás): o pedido para trás marca até onde a fonte já
        // foi consultada, mesmo que ela não tenha dado tão antigo.
        let mut ranges = Vec::new();
        match covered {
            None => ranges.push((from, today, true)),
            Some(covered) => {
                if from < covered {
                    ranges.push((from, covered - Duration::days(1), true));
                }
                let next = last.map_or(covered, |l| l + Duration::days(1));
                ranges.push((next.max(from), today, false));
            }
        }
        // Os desdobramentos vêm no mesmo pedido, de `start` até hoje, e vão
        // para `price_splits` (migração 0030, a mesma do comparador): o risco
        // mede o retorno no pé de hoje com eles. Sem a lista conferida desde
        // `from` (símbolo guardado antes dela), um pedido só, de `from` a hoje,
        // confere tudo; os fechamentos que já estavam guardados não mudam
        // (`docs/risco.md`).
        if self
            .store
            .split_check(symbol)
            .await?
            .is_none_or(|(checked_from, _)| checked_from > from)
        {
            ranges = vec![(from, today, true)];
        }
        let holders: Vec<Uuid> = holders.iter().copied().collect();
        let mut stored = 0;
        for (start, end, backward) in ranges.into_iter().filter(|(s, e, _)| s <= e) {
            match self.history.closes_and_splits(symbol, start, end).await {
                Ok((closes, splits)) => {
                    // Os eventos antes dos preços: se a gravação parar no meio,
                    // o próximo pedido começa antes do último preço guardado. O
                    // pedido vai até hoje, então a lista de `start` em diante é
                    // a da fonte, inteira.
                    self.store
                        .save_price_splits(symbol, start, &splits, now)
                        .await?;
                    let inserted = self.store.insert_prices(symbol, &closes).await?;
                    stored += inserted.len();
                    if backward {
                        self.store.set_price_coverage(symbol, start).await?;
                    }
                    if let Some(first) = inserted.first().filter(|d| **d < today) {
                        self.store.mark_dirty(&holders, *first).await?;
                    }
                }
                Err(HistoryError::NotFound) if backward => {
                    // A fonte não conhece o símbolo: não adianta pedir de novo.
                    self.store.set_price_coverage(symbol, start).await?;
                    self.store
                        .save_price_splits(symbol, start, &[], now)
                        .await?;
                }
                Err(error) => {
                    tracing::warn!(symbol = %symbol, error = %error, "fechamentos nao baixados");
                }
            }
        }
        Ok(stored)
    }

    /// O histórico dos títulos do Tesouro que faltam: um download só da
    /// tabela, para todos.
    async fn sync_bonds(
        &self,
        bonds: BTreeMap<Bond, BondNeed>,
        today: Date,
    ) -> Result<usize, StoreError> {
        if bonds.is_empty() {
            return Ok(0);
        }
        let wanted: BTreeMap<Bond, Date> = bonds.iter().map(|(b, n)| (*b, n.from)).collect();
        let history = match self.treasury.history(&wanted).await {
            Ok(history) => history,
            Err(error) => {
                tracing::warn!(error = %error, "historico do tesouro nao baixado");
                return Ok(0);
            }
        };
        let mut stored = 0;
        for (bond, need) in bonds {
            let series: Series = history.get(&bond).cloned().unwrap_or_default();
            let inserted = self.store.insert_prices(&need.symbol, &series).await?;
            stored += inserted.len();
            self.store
                .set_price_coverage(&need.symbol, need.from)
                .await?;
            if let Some(first) = inserted.first().filter(|d| **d < today) {
                let holders: Vec<Uuid> = need.holders.into_iter().collect();
                self.store.mark_dirty(&holders, *first).await?;
            }
        }
        Ok(stored)
    }

    /// Baixa o que falta de cada índice, desde um pouco antes da primeira
    /// foto; o Ibovespa, também desde o começo da janela mais longa do risco
    /// (a volatilidade e a queda dele ficam ao lado das da carteira, e as
    /// datas dele são os pregões). Sem nenhuma foto, só o CDI: os 12 meses
    /// dele são o alvo do FII de papel (`domain::market_rates`), então ele
    /// cobre sempre pelo menos [`CDI_LOOKBACK_DAYS`], com ou sem carteira.
    async fn sync_indexes(&self, today: Date) -> Result<usize, StoreError> {
        let first = self
            .store
            .first_snapshot_date()
            .await?
            .map(|first| first - Duration::days(INDEX_LOOKBACK_DAYS));
        let cdi_floor = today - Duration::days(CDI_LOOKBACK_DAYS);
        let mut stored = 0;
        for code in IndexCode::ALL {
            let start = match (code, first) {
                (IndexCode::Cdi, first) => first.map_or(cdi_floor, |f| f.min(cdi_floor)),
                (_, Some(first)) => first,
                (_, None) => continue,
            };
            let start = if code.monthly() {
                start.replace_day(1).unwrap_or(start)
            } else if code == IndexCode::Ibov {
                start.min(risk::history_start(today))
            } else {
                start
            };
            // O que falta antes do que já está guardado (a série pode passar a
            // começar mais cedo) e depois dele.
            let ranges = match self.store.index_bounds(code).await? {
                None => vec![(start, today)],
                Some((first, last)) => vec![
                    (start, first - Duration::days(1)),
                    (last + Duration::days(1), today),
                ],
            };
            for (from, to) in ranges.into_iter().filter(|(from, to)| from <= to) {
                match self.history.index(code, from, to).await {
                    Ok(values) => {
                        stored += values.len();
                        self.store.upsert_index(code, &values).await?;
                    }
                    Err(error) => {
                        tracing::warn!(index = code.as_str(), error = %error, "indice nao baixado");
                    }
                }
            }
        }
        Ok(stored)
    }

    /// Agenda a tarefa no horário `at` de Brasília, todo dia, até o processo
    /// encerrar. Se o processo sobe depois do horário, roda já (o deploy da
    /// noite não perde o dia; a tarefa é idempotente).
    pub fn spawn(self: Arc<Self>, at: Time) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut catch_up = past_todays_run(OffsetDateTime::now_utc(), at);
            loop {
                if !catch_up {
                    let now = OffsetDateTime::now_utc();
                    let wait = (next_run(now, at) - now).max(Duration::ZERO);
                    tokio::time::sleep(wait.unsigned_abs()).await;
                }
                catch_up = false;
                match self.run_daily(OffsetDateTime::now_utc()).await {
                    Ok(report) => tracing::info!(
                        ran = report.ran,
                        snapshots = report.snapshots,
                        failed = report.failed,
                        prices = report.prices,
                        indexes = report.indexes,
                        "tarefa diaria de desempenho"
                    ),
                    Err(error) => {
                        tracing::error!(error = %error, "tarefa diaria de desempenho falhou");
                    }
                }
            }
        })
    }

    /// A reconstrução em segundo plano: a cada `every`, refaz as séries com
    /// marca de "sujo". A rodada sem marca é uma consulta só.
    pub fn spawn_rebuilds(
        self: Arc<Self>,
        every: std::time::Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // Ao subir, uma conferência do cache de preços (o que foi gravado
            // antes do filtro de fechamento ruim); a tarefa diária repete.
            match self.clean_prices().await {
                Ok(Some(removed)) if removed > 0 => {
                    tracing::warn!(removed, "fechamentos ruins apagados do cache");
                }
                Ok(_) => {}
                Err(error) => tracing::error!(error = %error, "conferencia dos precos falhou"),
            }
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match self.run_pending(OffsetDateTime::now_utc()).await {
                    Ok(report) if report.users > 0 || report.failed > 0 => tracing::info!(
                        users = report.users,
                        days = report.days,
                        failed = report.failed,
                        prices = report.prices,
                        "serie do desempenho refeita"
                    ),
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(error = %error, "reconstrucao do desempenho falhou");
                    }
                }
            }
        })
    }
}

/// A moeda de um símbolo antes de ele ter cotação guardada.
pub(crate) fn default_currency(market: Market) -> &'static str {
    match market {
        Market::Us | Market::Crypto => "USD",
        Market::B3 | Market::Treasury | Market::Manual => "BRL",
    }
}

/// O símbolo do histórico de preço do ativo. `None` para quem não tem preço de
/// mercado: renda fixa por valor e ativo com preço manual (o manual vence a
/// cotação, como na tela).
pub(crate) fn price_symbol(view: &AssetView) -> Option<String> {
    if view.asset.manual_price.is_some() {
        return None;
    }
    let market = view.asset.market();
    view.asset
        .symbol
        .as_deref()
        .and_then(|s| market.quote_symbol(s))
}

/// A moeda do histórico do ativo: a da cotação ou, sem cotação ainda, a do
/// mercado.
pub(crate) fn price_currency(view: &AssetView) -> String {
    if view.price_source == PriceSource::Missing && view.currency == "BRL" {
        default_currency(view.asset.market()).to_owned()
    } else {
        view.currency.clone()
    }
}

/// O livro e os preços de cada ativo da pessoa de `from` a `to`, como a série
/// os vê, com a classe de cada um. No dia de hoje, o ativo sem fechamento vale
/// pelo preço que a tela mostra (cotação do momento ou preço manual).
pub(crate) async fn asset_histories(
    store: &Store,
    user: Uuid,
    from: Date,
    to: Date,
    now: OffsetDateTime,
) -> Result<Vec<(AssetClass, AssetHistory)>, StoreError> {
    let today = local_date(now);
    let view = service::portfolio(store, user).await?;
    let mut entries = store.ledger_entries(user).await?;
    let mut symbols: BTreeSet<String> = BTreeSet::new();
    for asset in &view.assets {
        if let Some(symbol) = price_symbol(asset) {
            symbols.insert(symbol);
            let currency = price_currency(asset);
            if currency != "BRL" {
                symbols.insert(fx_symbol(&currency));
            }
        }
    }
    let symbols: Vec<String> = symbols.into_iter().collect();
    let closes = store.price_series(&symbols, from, to).await?;
    let stored = store.snapshot_prices(user, from, to).await?;
    let empty = Series::new();
    Ok(view
        .assets
        .iter()
        .filter_map(|a| {
            let entries = entries.remove(&a.asset.id)?;
            let by_value = a.asset.market().unit_kind() == UnitKind::Value;
            let market_prices = price_symbol(a).map_or_else(Series::new, |symbol| {
                let currency = price_currency(a);
                let fx = (currency != "BRL")
                    .then(|| closes.get(&fx_symbol(&currency)).unwrap_or(&empty));
                retro::in_reais(closes.get(&symbol).unwrap_or(&empty), fx)
            });
            let mut prices =
                retro::merge_prices(&market_prices, stored.get(&a.asset.id).unwrap_or(&empty));
            if to == today && !by_value && prices.last().is_none_or(|(d, _)| *d < today) {
                if let Some(live) = a.unit_price.filter(|p| *p > Decimal::ZERO) {
                    prices.push((today, live));
                }
            }
            Some((
                a.asset.asset_class,
                AssetHistory {
                    asset_id: a.asset.id,
                    by_value,
                    entries,
                    prices,
                    fallback_price: if by_value { None } else { a.asset.manual_price },
                },
            ))
        })
        .collect())
}

/// Os dias de `from` a `to` da série da pessoa, calculados e não gravados.
/// `previous` é a foto do dia anterior a `from` (sem ela, `from` abre a série).
async fn compute_days(
    store: &Store,
    user: Uuid,
    from: Date,
    to: Date,
    previous: Option<&Snapshot>,
    now: OffsetDateTime,
) -> Result<Vec<Snapshot>, StoreError> {
    if from > to {
        return Ok(Vec::new());
    }
    let assets: Vec<AssetHistory> = asset_histories(store, user, from, to, now)
        .await?
        .into_iter()
        .map(|(_, history)| history)
        .collect();
    let income = store.received_income(user, from, to).await?;
    let start = previous.map(|p| Start {
        quota: p.quota,
        positions: &p.positions,
    });
    Ok(retro::rebuild(&assets, &income, start, from, to)
        .into_iter()
        .map(|day| Snapshot {
            date: day.date,
            market_value: day.market_value,
            net_flow: day.net_flow,
            income: day.income,
            quota: day.quota,
            positions: day.positions,
            taken_at: now,
        })
        .collect())
}

/// Refaz e grava a série da pessoa de `since` até `end`. Continua da foto
/// anterior a `since` quando ela existe; senão (ou se o livro passou a começar
/// antes), refaz desde o primeiro lançamento. O que ficou antes do primeiro
/// lançamento sai. Devolve quantos dias gravou.
pub async fn rebuild_series(
    store: &Store,
    user: Uuid,
    since: Date,
    end: Date,
    now: OffsetDateTime,
) -> Result<usize, StoreError> {
    let Some(first) = store.first_trade_date(user).await? else {
        store.clear_snapshots(user).await?;
        return Ok(0);
    };
    let mut from = since.max(first);
    let previous = if from > first {
        store
            .snapshot_before(user, from)
            .await?
            .filter(|p| p.date >= first)
    } else {
        None
    };
    match &previous {
        None => from = first,
        // Buraco na série (a tarefa ficou dias sem rodar): continua do dia
        // seguinte à última foto.
        Some(p) => from = from.min(p.date + Duration::days(1)),
    }
    let days = compute_days(store, user, from, end, previous.as_ref(), now).await?;
    store
        .replace_snapshots(user, Some(first), from, &days)
        .await?;
    Ok(days.len())
}

/// A foto de hoje, sem gravar: a série desde a última foto até hoje, com o
/// preço do momento onde ainda não há fechamento. `None` para quem não tem
/// livro nem série.
pub async fn compute_snapshot(
    store: &Store,
    user: Uuid,
    now: OffsetDateTime,
) -> Result<Option<Snapshot>, StoreError> {
    let today = local_date(now);
    let previous = store.snapshot_before(user, today).await?;
    let from = match &previous {
        Some(p) => p.date + Duration::days(1),
        None => match store.first_trade_date(user).await? {
            Some(first) => first,
            None => return Ok(None),
        },
    };
    let mut days = compute_days(store, user, from, today, previous.as_ref(), now).await?;
    Ok(days.pop())
}

/// Um ponto da série que a tela lê.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeriesPoint {
    pub date: Date,
    pub market_value: Decimal,
    pub net_flow: Decimal,
    pub income: Decimal,
    pub quota_value: Decimal,
    /// A foto de hoje calculada na hora, antes da tarefa do fechamento.
    pub partial: bool,
}

/// A série gravada mais a foto parcial de hoje, se a de hoje ainda não existe.
pub async fn series(
    store: &Store,
    user: Uuid,
    now: OffsetDateTime,
) -> Result<Vec<SeriesPoint>, StoreError> {
    let mut points: Vec<SeriesPoint> = store
        .snapshot_points(user)
        .await?
        .into_iter()
        .map(|p| SeriesPoint {
            date: p.date,
            market_value: p.market_value,
            net_flow: p.net_flow,
            income: p.income,
            quota_value: p.quota_value,
            partial: false,
        })
        .collect();
    let today = local_date(now);
    if points.last().is_none_or(|p| p.date < today) {
        if let Some(snapshot) = compute_snapshot(store, user, now).await? {
            points.push(SeriesPoint {
                date: snapshot.date,
                market_value: snapshot.market_value,
                net_flow: snapshot.net_flow,
                income: snapshot.income,
                quota_value: snapshot.quota.value,
                partial: true,
            });
        }
    }
    Ok(points)
}

pub(crate) fn quota_points(points: &[SeriesPoint]) -> Vec<QuotaPoint> {
    points
        .iter()
        .map(|p| QuotaPoint {
            date: p.date,
            value: p.quota_value,
        })
        .collect()
}

/// Patrimônio num dia, separado em aplicado e ganho.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetWorthPoint {
    pub date: Date,
    pub market_value: Decimal,
    /// A soma dos fluxos desde o começo da série: o que a pessoa pôs menos o
    /// que tirou.
    pub invested: Decimal,
    /// Patrimônio − aplicado: o ganho de preço (o provento não está nele).
    pub gain: Decimal,
    /// Proventos recebidos desde o começo da série.
    pub income: Decimal,
    pub partial: bool,
}

/// O patrimônio do período, a partir do fechamento de base.
pub fn net_worth(points: &[SeriesPoint], period: Period, today: Date) -> Vec<NetWorthPoint> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let from = window(points, period, today).map_or(first.date, |w| w.from);
    let mut invested = Decimal::ZERO;
    let mut income = Decimal::ZERO;
    let mut out = Vec::new();
    for p in points {
        invested += p.net_flow;
        income += p.income;
        if p.date >= from {
            out.push(NetWorthPoint {
                date: p.date,
                market_value: p.market_value,
                invested,
                gain: p.market_value - invested,
                income,
                partial: p.partial,
            });
        }
    }
    out
}

/// De onde veio a variação do patrimônio no período: aportes, mercado e
/// proventos (`domain::net_worth`). A base é o fechamento de base do período.
pub fn net_worth_bridge(points: &[SeriesPoint], period: Period, today: Date) -> Option<Bridge> {
    let base = window(points, period, today)?.from;
    let flows: Vec<FlowPoint> = points
        .iter()
        .filter(|p| p.date <= today)
        .map(|p| FlowPoint {
            date: p.date,
            market_value: p.market_value,
            net_flow: p.net_flow,
            income: p.income,
        })
        .collect();
    net_worth::bridge(&flows, base)
}

pub(crate) fn window(points: &[SeriesPoint], period: Period, today: Date) -> Option<Window> {
    let first = points.first()?;
    rules::period_return(
        &quota_points(points),
        period.start(today, first.date),
        today,
    )
}

/// Os índices guardados, prontos para acumular.
pub type Indexes = HashMap<IndexCode, Vec<(Date, Decimal)>>;

/// Lê do banco os índices pedidos, do começo da série (com folga) até hoje.
pub async fn load_indexes(
    store: &Store,
    codes: &[IndexCode],
    points: &[SeriesPoint],
    today: Date,
) -> Result<Indexes, StoreError> {
    let mut out = Indexes::new();
    let Some(first) = points.first() else {
        return Ok(out);
    };
    let from = (first.date - Duration::days(45))
        .replace_day(1)
        .unwrap_or(first.date);
    for code in codes.iter().copied().chain([IndexCode::Cdi]) {
        if let std::collections::hash_map::Entry::Vacant(slot) = out.entry(code) {
            slot.insert(store.index_series(code, from, today).await?);
        }
    }
    Ok(out)
}

pub fn accrue(code: IndexCode, values: &[(Date, Decimal)], from: Date, to: Date) -> Accrued {
    match code {
        IndexCode::Cdi | IndexCode::Selic => rules::daily_rate_accrued(values, from, to),
        IndexCode::Ipca => rules::monthly_rate_accrued(values, from, to),
        IndexCode::Ibov | IndexCode::Ifix => rules::level_accrued(values, from, to),
    }
}

/// Um índice no período.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BenchmarkReport {
    pub code: IndexCode,
    pub accrued: Accrued,
}

/// Um ponto da linha de rentabilidade: tudo acumulado desde a base.
#[derive(Debug, Clone, PartialEq)]
pub struct CurvePoint {
    pub date: Date,
    pub portfolio: Decimal,
    /// `None` onde o índice ainda não tem dado.
    pub benchmarks: Vec<(IndexCode, Option<Decimal>)>,
}

/// Um cartão "carteira contra o CDI".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Card {
    pub window: Window,
    pub cdi: Accrued,
    pub versus: Versus,
    /// A série cobre a janela inteira (12 meses de fato).
    pub full: bool,
}

/// Um índice num mês (ou num ano) da tabela: na janela dele e acumulado desde
/// o começo da série até o fim dele.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonthIndex {
    pub code: IndexCode,
    pub month: Accrued,
    pub accumulated: Accrued,
}

/// Um mês da tabela, com os índices pedidos nas mesmas janelas.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthReport {
    pub cell: MonthCell,
    pub benchmarks: Vec<MonthIndex>,
}

/// Um ano da tabela: os meses, o ano composto, o acumulado desde o começo
/// até o fim do ano, e os índices pedidos no ano e acumulados.
#[derive(Debug, Clone, PartialEq)]
pub struct YearReport {
    pub row: YearRow,
    /// O CDI na janela do ano (o campo de sempre do contrato).
    pub cdi: Accrued,
    /// Desde a primeira cota até o fim do ano, fração.
    pub accumulated: Decimal,
    pub months: Vec<MonthReport>,
    /// Cada índice pedido no ano e acumulado até o fim dele.
    pub benchmarks: Vec<MonthIndex>,
}

/// O resumo dos meses: melhor, pior, contagens e em quantos meses a carteira
/// ficou acima de cada índice pedido.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthsSummary {
    pub stats: MonthStats,
    pub versus: Vec<(IndexCode, Beat)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceReport {
    pub period: Period,
    pub starts_on: Option<Date>,
    pub window: Option<Window>,
    pub benchmarks: Vec<BenchmarkReport>,
    pub curve: Vec<CurvePoint>,
    pub twelve_months: Option<Card>,
    pub since_start: Option<Card>,
    pub years: Vec<YearReport>,
    pub months: MonthsSummary,
}

fn card(points: &[QuotaPoint], from: Date, today: Date, cdi: &[(Date, Decimal)]) -> Option<Card> {
    let window = rules::period_return(points, from, today)?;
    let accrued = rules::daily_rate_accrued(cdi, window.from, window.to);
    Some(Card {
        window,
        cdi: accrued,
        versus: rules::versus(window.ret, accrued.ret),
        full: window.from <= from,
    })
}

/// A rentabilidade do período contra os índices pedidos, os cartões de 12
/// meses e desde o início, e a tabela mês × ano.
pub fn performance(
    points: &[SeriesPoint],
    period: Period,
    codes: &[IndexCode],
    indexes: &Indexes,
    today: Date,
) -> PerformanceReport {
    let quotas = quota_points(points);
    let empty = Vec::new();
    let values = |code: IndexCode| indexes.get(&code).unwrap_or(&empty);
    let starts_on = points.first().map(|p| p.date);
    let window = window(points, period, today);
    let benchmarks = window
        .map(|w| {
            codes
                .iter()
                .map(|code| BenchmarkReport {
                    code: *code,
                    accrued: accrue(*code, values(*code), w.from, w.to),
                })
                .collect()
        })
        .unwrap_or_default();
    let curve = window
        .map(|w| {
            quotas
                .iter()
                .filter(|q| q.date >= w.from && q.date <= w.to)
                .map(|q| {
                    let base = quotas
                        .iter()
                        .find(|b| b.date == w.from)
                        .map_or(q.value, |b| b.value);
                    CurvePoint {
                        date: q.date,
                        portfolio: if base > Decimal::ZERO {
                            q.value / base - Decimal::ONE
                        } else {
                            Decimal::ZERO
                        },
                        benchmarks: codes
                            .iter()
                            .map(|code| {
                                let accrued = accrue(*code, values(*code), w.from, q.date);
                                let known = q.date == w.from || accrued.as_of.is_some();
                                (*code, known.then_some(accrued.ret))
                            })
                            .collect(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let cdi = values(IndexCode::Cdi);
    let twelve_months = starts_on.and_then(|_| {
        card(
            &quotas,
            Period::TwelveMonths.start(today, today),
            today,
            cdi,
        )
    });
    let since_start = starts_on.and_then(|first| card(&quotas, first, today, cdi));
    let (years, months) = year_reports(&quotas, codes, indexes);
    PerformanceReport {
        period,
        starts_on,
        window,
        benchmarks,
        curve,
        twelve_months,
        since_start,
        years,
        months,
    }
}

/// A tabela mês × ano com os índices nas mesmas janelas, e o resumo dos
/// meses. O acumulado conta da primeira cota.
fn year_reports(
    quotas: &[QuotaPoint],
    codes: &[IndexCode],
    indexes: &Indexes,
) -> (Vec<YearReport>, MonthsSummary) {
    let empty = Vec::new();
    let values = |code: IndexCode| indexes.get(&code).unwrap_or(&empty);
    let Some(origin) = quotas.first().copied() else {
        return (
            Vec::new(),
            MonthsSummary {
                stats: MonthStats::default(),
                versus: Vec::new(),
            },
        );
    };
    let index_at = |from: Date, to: Date| -> Vec<MonthIndex> {
        codes
            .iter()
            .map(|code| MonthIndex {
                code: *code,
                month: accrue(*code, values(*code), from, to),
                accumulated: accrue(*code, values(*code), origin.date, to),
            })
            .collect()
    };
    let cells = rules::monthly_cells(quotas);
    let months: Vec<MonthReport> = cells
        .iter()
        .map(|cell| MonthReport {
            cell: *cell,
            benchmarks: index_at(cell.from, cell.to),
        })
        .collect();
    let cdi = values(IndexCode::Cdi);
    let years = rules::monthly_table(quotas)
        .into_iter()
        .map(|row| {
            let year: Vec<MonthReport> = months
                .iter()
                .filter(|m| m.cell.year == row.year)
                .cloned()
                .collect();
            YearReport {
                cdi: rules::daily_rate_accrued(cdi, row.from, row.to),
                accumulated: year.last().map_or(Decimal::ZERO, |m| m.cell.accumulated),
                months: year,
                benchmarks: index_at(row.from, row.to),
                row,
            }
        })
        .collect();
    let versus = codes
        .iter()
        .enumerate()
        .map(|(i, code)| {
            let index: Vec<Option<Decimal>> = months
                .iter()
                .map(|m| {
                    let accrued = m.benchmarks[i].month;
                    accrued.as_of.map(|_| accrued.ret)
                })
                .collect();
            (*code, monthly_stats::months_above(&cells, &index))
        })
        .collect();
    (
        years,
        MonthsSummary {
            stats: monthly_stats::month_stats(&cells),
            versus,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::{date, datetime, time};
    use time::Month;

    #[test]
    fn schedule_follows_brasilia_time() {
        let at = time!(19:00);
        // 21:30 UTC = 18:30 em Brasília: hoje às 19:00 (22:00 UTC).
        let before = datetime!(2026-11-25 21:30 UTC);
        assert_eq!(next_run(before, at), datetime!(2026-11-25 22:00 UTC));
        assert!(!past_todays_run(before, at));
        // 23:00 UTC = 20:00: amanhã.
        let after = datetime!(2026-11-25 23:00 UTC);
        assert_eq!(next_run(after, at), datetime!(2026-11-26 22:00 UTC));
        assert!(past_todays_run(after, at));
        // 01:00 UTC de 26 ainda é 25 em Brasília.
        assert_eq!(
            local_date(datetime!(2026-11-26 01:00 UTC)),
            date!(2026 - 11 - 25)
        );
    }

    fn point(date: Date, market_value: Decimal, net_flow: Decimal, quota: Decimal) -> SeriesPoint {
        SeriesPoint {
            date,
            market_value,
            net_flow,
            income: Decimal::ZERO,
            quota_value: quota,
            partial: false,
        }
    }

    #[test]
    fn net_worth_splits_invested_and_gain() {
        let points = [
            point(date!(2026 - 10 - 30), dec!(1000), dec!(1000), dec!(1)),
            point(date!(2026 - 11 - 02), dec!(1530), dec!(500), dec!(1.03)),
            point(date!(2026 - 11 - 25), dec!(1600), dec!(0), dec!(1.077)),
        ];
        let all = net_worth(&points, Period::All, date!(2026 - 11 - 25));
        assert_eq!(all.len(), 3);
        assert_eq!(all[1].invested, dec!(1500));
        assert_eq!(all[2].gain, dec!(100));
        // O mês começa no fechamento de base (30/10), com o aplicado de lá.
        let month = net_worth(&points, Period::Month, date!(2026 - 11 - 25));
        assert_eq!(month.len(), 3);
        assert!(net_worth(&[], Period::All, date!(2026 - 11 - 25)).is_empty());
    }

    #[test]
    fn performance_compares_with_the_indexes() {
        let points = [
            point(date!(2026 - 11 - 23), dec!(1000), dec!(1000), dec!(1)),
            point(date!(2026 - 11 - 24), dec!(1001), dec!(0), dec!(1.001)),
            point(date!(2026 - 11 - 25), dec!(1002), dec!(0), dec!(1.002)),
        ];
        let rate = dec!(0.050788);
        let indexes: Indexes = [
            (
                IndexCode::Cdi,
                vec![(date!(2026 - 11 - 23), rate), (date!(2026 - 11 - 24), rate)],
            ),
            (IndexCode::Ibov, vec![(date!(2026 - 11 - 23), dec!(164796))]),
        ]
        .into_iter()
        .collect();
        let codes = [IndexCode::Cdi, IndexCode::Ibov, IndexCode::Ipca];
        let report = performance(
            &points,
            Period::All,
            &codes,
            &indexes,
            date!(2026 - 11 - 25),
        );
        assert_eq!(report.starts_on, Some(date!(2026 - 11 - 23)));
        assert_eq!(report.window.map(|w| w.ret), Some(dec!(0.002)));
        assert_eq!(report.curve.len(), 3);
        assert_eq!(
            report.curve[0].benchmarks[0],
            (IndexCode::Cdi, Some(Decimal::ZERO))
        );
        assert!(report.curve[2].benchmarks[0].1.is_some());
        // Sem IPCA publicado na janela: a linha some, não vira zero.
        assert_eq!(report.curve[2].benchmarks[2], (IndexCode::Ipca, None));
        let start = report.since_start.unwrap();
        assert!(start.full);
        assert!(start.versus.excess > Decimal::ZERO);
        let year = report.twelve_months.unwrap();
        assert!(!year.full);
        assert_eq!(report.years.len(), 1);
        assert_eq!(report.benchmarks.len(), 3);

        let empty = performance(
            &[],
            Period::All,
            &codes,
            &Indexes::new(),
            date!(2026 - 11 - 25),
        );
        assert!(empty.window.is_none() && empty.curve.is_empty() && empty.since_start.is_none());
        assert!(empty.years.is_empty() && empty.months.stats.best.is_none());
    }

    /// Ano a ano: cada mês contra o índice na mesma janela, o acumulado desde
    /// o começo e em quantos meses a carteira ficou acima.
    #[test]
    fn year_by_year_compares_each_month_with_the_indexes() {
        let points = [
            point(date!(2026 - 10 - 30), dec!(1000), dec!(1000), dec!(1)),
            point(date!(2026 - 10 - 31), dec!(1010), dec!(0), dec!(1.01)),
            point(date!(2026 - 11 - 15), dec!(1000), dec!(0), dec!(1)),
            point(date!(2026 - 11 - 30), dec!(1030), dec!(0), dec!(1.03)),
        ];
        let indexes: Indexes = [(
            IndexCode::Ifix,
            vec![
                (date!(2026 - 10 - 30), dec!(3000)),
                (date!(2026 - 10 - 31), dec!(3030)),
                (date!(2026 - 11 - 30), dec!(3060.3)),
            ],
        )]
        .into_iter()
        .collect();
        let codes = [IndexCode::Ifix, IndexCode::Ipca];
        let report = performance(
            &points,
            Period::All,
            &codes,
            &indexes,
            date!(2026 - 11 - 30),
        );
        assert_eq!(report.years.len(), 1);
        let year = &report.years[0];
        assert_eq!(year.months.len(), 2);
        assert_eq!(year.accumulated, dec!(0.03));
        let (october, november) = (&year.months[0], &year.months[1]);
        assert_eq!(october.cell.ret, dec!(0.01));
        assert_eq!(october.benchmarks[0].month.ret, dec!(0.01));
        assert_eq!(november.cell.from, date!(2026 - 10 - 31));
        assert_eq!(
            november.benchmarks[0].month.ret,
            dec!(3060.3) / dec!(3030) - Decimal::ONE
        );
        assert_eq!(
            november.benchmarks[0].accumulated.ret,
            dec!(3060.3) / dec!(3000) - Decimal::ONE
        );
        // Sem IPCA publicado: nada a comparar.
        assert_eq!(november.benchmarks[1].month.as_of, None);
        assert_eq!(
            year.benchmarks[0].month.ret,
            dec!(3060.3) / dec!(3000) - Decimal::ONE
        );
        let stats = report.months.stats;
        assert_eq!((stats.positive, stats.negative), (2, 0));
        assert_eq!(stats.best.map(|b| b.month), Some(Month::November));
        assert_eq!(stats.worst.map(|w| w.month), Some(Month::October));
        // Outubro empatou com o IFIX (1% contra 1%): só novembro fica acima.
        assert_eq!(
            report.months.versus,
            vec![
                (
                    IndexCode::Ifix,
                    Beat {
                        above: 1,
                        compared: 2
                    }
                ),
                (
                    IndexCode::Ipca,
                    Beat {
                        above: 0,
                        compared: 0
                    }
                ),
            ]
        );
    }

    #[test]
    fn net_worth_bridge_follows_the_period() {
        let points = [
            point(date!(2025 - 10 - 30), dec!(1000), dec!(1000), dec!(1)),
            point(date!(2026 - 10 - 31), dec!(1200), dec!(100), dec!(1.09)),
            point(date!(2026 - 11 - 25), dec!(1300), dec!(50), dec!(1.135)),
        ];
        let today = date!(2026 - 11 - 25);
        // O mês mede do fechamento de 31/10: o aporte de 50 e +50 do mercado.
        let month = net_worth_bridge(&points, Period::Month, today).unwrap();
        assert_eq!(month.from, Some(date!(2026 - 10 - 31)));
        assert_eq!(
            (month.start_value, month.contributions, month.market),
            (dec!(1200), dec!(50), dec!(50))
        );
        assert_eq!((month.invested, month.gain), (dec!(1150), dec!(150)));
        // Cinco anos: a série é mais nova, começa do zero.
        let five = net_worth_bridge(&points, Period::FiveYears, today).unwrap();
        assert_eq!(five.from, None);
        assert_eq!((five.contributions, five.market), (dec!(1150), dec!(150)));
        assert!(net_worth_bridge(&[], Period::All, today).is_none());
    }
}
