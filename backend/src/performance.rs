//! Desempenho: a tarefa diária (fechamentos, foto da carteira, índices) e o
//! que as telas de patrimônio e rentabilidade leem. A regra está em
//! [`crate::domain::performance`]; o método e os limites, em
//! `docs/desempenho.md`.
//!
//! Com duas réplicas, as duas agendam a tarefa no mesmo horário. Quem roda é
//! quem pega o advisory lock [`JOB_LOCK_KEY`] (`pg_try_advisory_lock`); a outra
//! vê o lock ocupado e pula. Mesmo que as duas rodassem uma depois da outra, a
//! foto é um `upsert` por (pessoa, dia) e os caches só inserem data nova: a
//! segunda rodada refaz o mesmo resultado, sem duplicar.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use rust_decimal::Decimal;
use time::macros::offset;
use time::{Date, Duration, OffsetDateTime, Time, UtcOffset};
use uuid::Uuid;

use crate::domain::asset_class::UnitKind;
use crate::domain::performance::{
    self as rules, Accrued, Period, Position, QuotaPoint, Versus, Window, YearRow,
};
use crate::history::{HistoryProvider, IndexCode};
use crate::quotes::fx_symbol;
use crate::service::{self, PortfolioView};
use crate::store::{Snapshot, Store, StoreError};

/// Número do advisory lock da tarefa diária ("PRUMO" + migração 0006).
pub const JOB_LOCK_KEY: i64 = 0x50_52_55_4d_4f_06;
/// Horário de Brasília (sem horário de verão desde 2019).
pub const BRASILIA: UtcOffset = offset!(-3);
/// Primeiro download de um símbolo: uma semana para trás basta, a série da
/// carteira nasce hoje (a retroativa vem com o livro de lançamentos).
const PRICE_LOOKBACK_DAYS: i64 = 7;
/// Os índices começam um pouco antes da primeira foto, para haver base.
const INDEX_LOOKBACK_DAYS: i64 = 10;

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
    pub snapshots: usize,
    pub failed: usize,
    pub prices: usize,
    pub indexes: usize,
}

pub struct Performance {
    store: Store,
    history: Arc<dyn HistoryProvider>,
}

impl Performance {
    pub fn new(store: Store, history: Arc<dyn HistoryProvider>) -> Self {
        Self { store, history }
    }

    /// A tarefa do dia: fechamentos dos ativos, a foto de cada carteira e os
    /// índices. Só roda com o lock; sem ele, volta `ran: false`.
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
            prices: self.sync_prices(today).await?,
            ..JobReport::default()
        };
        for user in self.store.snapshot_users().await? {
            match take_snapshot(&self.store, user, now).await {
                Ok(Some(_)) => report.snapshots += 1,
                Ok(None) => {}
                Err(error) => {
                    report.failed += 1;
                    tracing::error!(error = %error, "foto da carteira nao gravada");
                }
            }
        }
        report.indexes = self.sync_indexes(today).await?;
        Ok(report)
    }

    /// Baixa o que falta do fechamento de cada símbolo cotado de alguma
    /// carteira (e do câmbio das moedas deles). Devolve quantos dias entraram.
    async fn sync_prices(&self, today: Date) -> Result<usize, StoreError> {
        let mut symbols: BTreeSet<String> = self
            .store
            .tracked_symbols()
            .await?
            .into_iter()
            .filter_map(|(market, symbol)| market.quote_symbol(&symbol))
            .collect();
        let listed: Vec<String> = symbols.iter().cloned().collect();
        symbols.extend(
            self.store
                .quote_currencies(&listed)
                .await?
                .iter()
                .filter(|c| c.as_str() != "BRL")
                .map(|c| fx_symbol(c)),
        );
        let mut stored = 0;
        for symbol in symbols {
            let from = match self.store.last_price_date(&symbol).await? {
                Some(last) => last + Duration::days(1),
                None => today - Duration::days(PRICE_LOOKBACK_DAYS),
            };
            if from > today {
                continue;
            }
            match self.history.closes(&symbol, from, today).await {
                Ok(closes) => {
                    stored += closes.len();
                    self.store.insert_prices(&symbol, &closes).await?;
                }
                Err(error) => {
                    tracing::warn!(symbol = %symbol, error = %error, "fechamentos nao baixados");
                }
            }
        }
        Ok(stored)
    }

    /// Baixa o que falta de cada índice, desde um pouco antes da primeira
    /// foto. Sem nenhuma foto, não baixa nada.
    async fn sync_indexes(&self, today: Date) -> Result<usize, StoreError> {
        let Some(first) = self.store.first_snapshot_date().await? else {
            return Ok(0);
        };
        let start = first - Duration::days(INDEX_LOOKBACK_DAYS);
        let mut stored = 0;
        for code in IndexCode::ALL {
            let start = if code.monthly() {
                start.replace_day(1).unwrap_or(start)
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
}

/// As posições da carteira no dia: o fechamento de `price_history` quando
/// existe para a data (com o câmbio do mesmo dia), senão o preço que a tela
/// mostra (cotação de agora ou preço manual).
fn positions(view: &PortfolioView, closes: &HashMap<String, Decimal>) -> Vec<Position> {
    view.assets
        .iter()
        .map(|a| {
            let market = a.asset.market();
            if market.unit_kind() == UnitKind::Value {
                return Position {
                    asset_id: a.asset.id,
                    by_value: true,
                    quantity: a.asset.quantity,
                    unit_price: None,
                };
            }
            let close = a
                .asset
                .symbol
                .as_deref()
                .filter(|_| a.asset.manual_price.is_none())
                .and_then(|s| market.quote_symbol(s))
                .and_then(|s| closes.get(&s).copied())
                .and_then(|close| {
                    if a.currency == "BRL" {
                        Some(close)
                    } else {
                        closes
                            .get(&fx_symbol(&a.currency))
                            .map(|fx| (close * fx).round_dp(8))
                    }
                });
            Position {
                asset_id: a.asset.id,
                by_value: false,
                quantity: a.asset.quantity,
                unit_price: close.or(a.unit_price).filter(|p| *p > Decimal::ZERO),
            }
        })
        .collect()
}

/// A foto de agora, sem gravar. `None` para quem nunca teve ativo nem série.
pub async fn compute_snapshot(
    store: &Store,
    user: Uuid,
    now: OffsetDateTime,
) -> Result<Option<Snapshot>, StoreError> {
    let today = local_date(now);
    let previous = store.snapshot_before(user, today).await?;
    let view = service::portfolio(store, user).await?;
    if view.assets.is_empty() && previous.is_none() {
        return Ok(None);
    }
    let symbols: Vec<String> = view.quote_symbols().into_iter().collect();
    let closes = store.closes_on(&symbols, today).await?;
    let positions = positions(&view, &closes);
    let market_value = rules::market_value(&positions);
    let (net_flow, quota) = match &previous {
        None => (
            market_value,
            rules::advance(None, market_value, market_value),
        ),
        Some(previous) => {
            let purchases = store
                .purchases_between(user, previous.taken_at, now)
                .await?;
            let flow = rules::net_flow(&previous.positions, &positions, &purchases);
            (
                flow,
                rules::advance(Some(previous.quota), market_value, flow),
            )
        }
    };
    Ok(Some(Snapshot {
        date: today,
        market_value,
        net_flow,
        quota,
        positions,
        taken_at: now,
    }))
}

/// A foto do dia, gravada (ou refeita, se já existia).
pub async fn take_snapshot(
    store: &Store,
    user: Uuid,
    now: OffsetDateTime,
) -> Result<Option<Snapshot>, StoreError> {
    let snapshot = compute_snapshot(store, user, now).await?;
    if let Some(snapshot) = &snapshot {
        store.upsert_snapshot(user, snapshot).await?;
    }
    Ok(snapshot)
}

/// Um ponto da série que a tela lê.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeriesPoint {
    pub date: Date,
    pub market_value: Decimal,
    pub net_flow: Decimal,
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
                quota_value: snapshot.quota.value,
                partial: true,
            });
        }
    }
    Ok(points)
}

fn quota_points(points: &[SeriesPoint]) -> Vec<QuotaPoint> {
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
    /// A soma dos fluxos desde o começo da série (a primeira foto entra
    /// inteira como aplicado).
    pub invested: Decimal,
    pub gain: Decimal,
    pub partial: bool,
}

/// O patrimônio do período, a partir do fechamento de base.
pub fn net_worth(points: &[SeriesPoint], period: Period, today: Date) -> Vec<NetWorthPoint> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let from = window(points, period, today).map_or(first.date, |w| w.from);
    let mut invested = Decimal::ZERO;
    let mut out = Vec::new();
    for p in points {
        invested += p.net_flow;
        if p.date >= from {
            out.push(NetWorthPoint {
                date: p.date,
                market_value: p.market_value,
                invested,
                gain: p.market_value - invested,
                partial: p.partial,
            });
        }
    }
    out
}

fn window(points: &[SeriesPoint], period: Period, today: Date) -> Option<Window> {
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
        IndexCode::Ibov => rules::level_accrued(values, from, to),
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

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceReport {
    pub period: Period,
    pub starts_on: Option<Date>,
    pub window: Option<Window>,
    pub benchmarks: Vec<BenchmarkReport>,
    pub curve: Vec<CurvePoint>,
    pub twelve_months: Option<Card>,
    pub since_start: Option<Card>,
    /// Cada ano com o CDI da mesma janela.
    pub years: Vec<(YearRow, Accrued)>,
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
    let years = rules::monthly_table(&quotas)
        .into_iter()
        .map(|row| {
            let accrued = rules::daily_rate_accrued(cdi, row.from, row.to);
            (row, accrued)
        })
        .collect();
    PerformanceReport {
        period,
        starts_on,
        window,
        benchmarks,
        curve,
        twelve_months,
        since_start,
        years,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::{date, datetime, time};

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
    }
}
