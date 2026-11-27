//! O mapa de calor da Renda: mês × ano, em reais, em DY e em yield on cost.
//!
//! - **R$**: o provento do mês (recebido e a receber), líquido ou bruto, pela
//!   data de pagamento ou pela data com — as mesmas escolhas do histórico.
//! - **DY do mês** = provento do mês ÷ **patrimônio médio do mês** × 100. O
//!   patrimônio médio é a média das fotos diárias da carteira inteira
//!   (`portfolio_snapshots.market_value`, uma por dia corrido desde o primeiro
//!   lançamento do livro), inclusive o que não paga provento (cripto, Tesouro):
//!   é quanto a carteira toda rendeu em provento naquele mês. A média, e não a
//!   foto do fim do mês, para um aporte grande no dia 30 não derrubar o DY do
//!   mês inteiro. Mês sem foto (antes do primeiro lançamento, ou depois de hoje)
//!   não tem DY.
//! - **Yield on cost do mês** = provento do mês ÷ **custo médio do mês** × 100.
//!   O custo é o `cost_basis` do livro (preço médio × quantidade, com taxas),
//!   em média nos dias do mês em que havia posição, até hoje. O saldo inicial
//!   vale para trás, a mesma regra da quantidade na data com
//!   (`entitlement::normalized`): o provento que ele gerou antes do cadastro
//!   tem custo para dividir.
//! - **O ano** soma o provento dos meses com base e divide pela média da base
//!   desses meses: o DY (ou o yield on cost) daquele ano, nunca anualizado.
//!   Mês depois do de hoje não entra.
//!
//! **Fica de fora da conta**, e a célula diz que é parcial: o provento de ativo
//! apagado da carteira (o patrimônio e o custo dele saíram junto) e, no yield
//! on cost, o de ativo com custo desconhecido naquele mês (saldo inicial sem
//! preço médio) — a mesma regra do yield on cost da carteira (`domain::yoc`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rust_decimal::Decimal;
use time::{Date, Duration, Month};
use uuid::Uuid;

use super::{AmountKind, DateBasis, HistoryEvent};
use crate::domain::entitlement::normalized;
use crate::domain::income::YearMonth;
use crate::domain::ledger::{replay, Entry, LedgerError, Position};

/// Casas do percentual: o DY de um mês é pequeno (0,3% a 1,5%), e duas casas
/// depois da vírgula ainda dizem a diferença entre dois meses.
const PERCENT_PLACES: u32 = 4;

fn percent(part: Decimal, whole: Decimal) -> Option<Decimal> {
    (whole > Decimal::ZERO).then(|| (part * Decimal::ONE_HUNDRED / whole).round_dp(PERCENT_PLACES))
}

/// O custo de um mês.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MonthCost {
    /// O custo médio, nos dias do mês com posição, dos ativos de custo
    /// conhecido o mês inteiro.
    pub cost: Decimal,
    /// Os ativos com posição e custo desconhecido em algum dia do mês: a renda
    /// deles sai do yield on cost daquele mês.
    pub unknown: BTreeSet<Uuid>,
}

/// O custo mês a mês.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MonthlyCosts {
    pub months: BTreeMap<YearMonth, MonthCost>,
    /// Ativos cujo livro não fecha (venda além da posição): o custo deles é
    /// desconhecido em todo mês, e quem chama registra no log.
    pub broken: Vec<Uuid>,
}

impl MonthlyCosts {
    fn unknown(&self, month: YearMonth, asset: Uuid) -> bool {
        self.broken.contains(&asset)
            || self
                .months
                .get(&month)
                .is_some_and(|m| m.unknown.contains(&asset))
    }

    fn cost(&self, month: YearMonth) -> Option<Decimal> {
        self.months
            .get(&month)
            .map(|m| m.cost)
            .filter(|c| *c > Decimal::ZERO)
    }
}

/// O primeiro dia de um mês.
pub fn first_day(month: YearMonth) -> Option<Date> {
    let m = Month::try_from(month.month).ok()?;
    Date::from_calendar_date(month.year, m, 1).ok()
}

/// O último dia de um mês.
pub fn last_day(month: YearMonth) -> Option<Date> {
    first_day(month.shift(1)).map(|d| d - Duration::days(1))
}

/// Os passos do livro de um ativo: a posição depois de cada dia com lançamento.
fn positions(entries: &[Entry]) -> Result<Vec<(Date, Position)>, LedgerError> {
    let steps = replay(&normalized(entries))?;
    let mut by_day: Vec<(Date, Position)> = Vec::with_capacity(steps.len());
    for step in steps {
        match by_day.last_mut() {
            Some((day, position)) if *day == step.entry.trade_date => *position = step.position,
            _ => by_day.push((step.entry.trade_date, step.position)),
        }
    }
    Ok(by_day)
}

/// O custo médio de cada mês de `from` a `to` (inclusive), até `today`, a
/// partir do livro de cada ativo.
pub fn monthly_costs(
    ledgers: &HashMap<Uuid, Vec<Entry>>,
    from: YearMonth,
    to: YearMonth,
    today: Date,
) -> MonthlyCosts {
    let mut broken = Vec::new();
    let mut books: Vec<(Uuid, Vec<(Date, Position)>)> = Vec::new();
    let mut list: Vec<(&Uuid, &Vec<Entry>)> = ledgers.iter().collect();
    list.sort_by_key(|(id, _)| **id);
    for (id, entries) in list {
        match positions(entries) {
            Ok(steps) => books.push((*id, steps)),
            Err(_) => broken.push(*id),
        }
    }
    let (Some(start), Some(end)) = (first_day(from), last_day(to).map(|d| d.min(today))) else {
        return MonthlyCosts {
            months: BTreeMap::new(),
            broken,
        };
    };

    // Por mês: a soma do custo de cada ativo nos dias com posição, quem teve
    // custo desconhecido e quantos dias a carteira teve alguma posição.
    struct Acc {
        sums: Vec<Decimal>,
        unknown: BTreeSet<usize>,
        days: u32,
    }
    let mut accs: BTreeMap<YearMonth, Acc> = BTreeMap::new();
    let mut cursors = vec![0usize; books.len()];
    let mut day = start;
    while day <= end {
        let month = YearMonth::of(day);
        let acc = accs.entry(month).or_insert_with(|| Acc {
            sums: vec![Decimal::ZERO; books.len()],
            unknown: BTreeSet::new(),
            days: 0,
        });
        let mut held = false;
        for (index, (_, steps)) in books.iter().enumerate() {
            let cursor = &mut cursors[index];
            while steps.get(*cursor).is_some_and(|(d, _)| *d <= day) {
                *cursor += 1;
            }
            let Some((_, position)) = cursor.checked_sub(1).and_then(|i| steps.get(i)) else {
                continue;
            };
            if position.quantity <= Decimal::ZERO {
                continue;
            }
            held = true;
            match (position.cost_basis, acc.sums.get_mut(index)) {
                (Some(cost), Some(sum)) => *sum += cost,
                _ => {
                    acc.unknown.insert(index);
                }
            }
        }
        if held {
            acc.days += 1;
        }
        let Some(next) = day.next_day() else {
            break;
        };
        day = next;
    }

    let months = accs
        .into_iter()
        .filter(|(_, acc)| acc.days > 0)
        .map(|(month, acc)| {
            let known: Decimal = acc
                .sums
                .iter()
                .enumerate()
                .filter(|(i, _)| !acc.unknown.contains(i))
                .map(|(_, sum)| *sum)
                .sum();
            let unknown = acc
                .unknown
                .iter()
                .filter_map(|i| books.get(*i).map(|(id, _)| *id))
                .collect();
            (
                month,
                MonthCost {
                    cost: (known / Decimal::from(acc.days)).round_dp(2),
                    unknown,
                },
            )
        })
        .collect();
    MonthlyCosts { months, broken }
}

/// Uma célula do mapa: um mês, ou o total do ano.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HeatCell {
    /// Provento do mês (recebido e a receber), pelo valor escolhido.
    pub amount: Decimal,
    /// A parte de `amount` que ainda vai cair.
    pub upcoming: Decimal,
    /// O patrimônio médio do mês; no ano, a média dos meses com foto.
    pub market_value: Option<Decimal>,
    /// `amount` (sem o que fica de fora) ÷ `market_value` × 100.
    pub dy: Option<Decimal>,
    /// Algum provento ficou fora do DY.
    pub dy_partial: bool,
    /// O custo médio do mês; no ano, a média dos meses com custo.
    pub cost: Option<Decimal>,
    pub yield_on_cost: Option<Decimal>,
    pub yield_on_cost_partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatYear {
    pub year: i32,
    /// Janeiro a dezembro.
    pub months: [HeatCell; 12],
    pub total: HeatCell,
}

/// Os meses que o mapa cobre: de janeiro do ano do primeiro provento a
/// dezembro do ano mais novo (o de hoje, ou o do último a receber). `None` sem
/// provento.
pub fn range(
    events: &[HistoryEvent],
    basis: DateBasis,
    today: Date,
) -> Option<(YearMonth, YearMonth)> {
    let first = events.iter().map(|e| e.date(basis)).min()?;
    let last = events
        .iter()
        .map(|e| e.date(basis))
        .max()
        .unwrap_or(first)
        .max(today);
    Some((
        YearMonth {
            year: first.year(),
            month: 1,
        },
        YearMonth {
            year: last.year(),
            month: 12,
        },
    ))
}

/// A soma de um mês, antes de dividir.
#[derive(Default)]
struct MonthSum {
    amount: Decimal,
    upcoming: Decimal,
    /// O que entra no DY: o provento de ativo que ainda está na carteira.
    kept: Decimal,
    /// O que entra no yield on cost: o de cima, menos o de custo desconhecido.
    known_cost: Decimal,
    removed: bool,
    unknown_cost: bool,
}

/// O mapa de calor, do ano mais novo ao mais velho. `market_values` é o
/// patrimônio médio de cada mês com foto.
pub fn heatmap(
    events: &[HistoryEvent],
    basis: DateBasis,
    amount: AmountKind,
    today: Date,
    market_values: &BTreeMap<YearMonth, Decimal>,
    costs: &MonthlyCosts,
) -> Vec<HeatYear> {
    let Some((from, to)) = range(events, basis, today) else {
        return Vec::new();
    };
    let mut sums: BTreeMap<YearMonth, MonthSum> = BTreeMap::new();
    for event in events {
        let month = YearMonth::of(event.date(basis));
        let value = event.amount(amount);
        let sum = sums.entry(month).or_default();
        sum.amount += value;
        if event.upcoming(today) {
            sum.upcoming += value;
        }
        match event.asset_id {
            None => sum.removed |= value > Decimal::ZERO,
            Some(asset) => {
                sum.kept += value;
                if costs.unknown(month, asset) {
                    sum.unknown_cost |= value > Decimal::ZERO;
                } else {
                    sum.known_cost += value;
                }
            }
        }
    }

    let current = YearMonth::of(today);
    let mut years = Vec::new();
    for year in (from.year..=to.year).rev() {
        let mut months: [HeatCell; 12] = Default::default();
        // O ano: soma dos numeradores e das bases dos meses que têm base.
        let mut dy_year = YearPart::default();
        let mut yoc_year = YearPart::default();
        let mut total = HeatCell::default();
        for (index, cell) in months.iter_mut().enumerate() {
            let month = YearMonth {
                year,
                month: u8::try_from(index + 1).unwrap_or(1),
            };
            let sum = sums.remove(&month).unwrap_or_default();
            let market_value = market_values
                .get(&month)
                .copied()
                .filter(|v| *v > Decimal::ZERO)
                .map(|v| v.round_dp(2));
            let cost = costs.cost(month);
            *cell = HeatCell {
                amount: sum.amount,
                upcoming: sum.upcoming,
                market_value,
                dy: market_value.and_then(|v| percent(sum.kept, v)),
                dy_partial: sum.removed,
                cost,
                yield_on_cost: cost.and_then(|c| percent(sum.known_cost, c)),
                yield_on_cost_partial: sum.removed || sum.unknown_cost,
            };
            total.amount += sum.amount;
            total.upcoming += sum.upcoming;
            if month <= current {
                dy_year.add(market_value, sum.kept, sum.amount, cell.dy_partial);
                yoc_year.add(cost, sum.known_cost, sum.amount, cell.yield_on_cost_partial);
            }
        }
        (total.market_value, total.dy, total.dy_partial) = dy_year.result();
        (total.cost, total.yield_on_cost, total.yield_on_cost_partial) = yoc_year.result();
        years.push(HeatYear {
            year,
            months,
            total,
        });
    }
    years
}

/// O acumulado do ano de um dos dois percentuais.
#[derive(Default)]
struct YearPart {
    numerator: Decimal,
    base: Decimal,
    months: u32,
    partial: bool,
}

impl YearPart {
    /// Um mês: com base, entra; sem base e com provento, o ano fica parcial.
    fn add(&mut self, base: Option<Decimal>, counted: Decimal, amount: Decimal, partial: bool) {
        match base {
            Some(base) => {
                self.numerator += counted;
                self.base += base;
                self.months += 1;
                self.partial |= partial;
            }
            None => self.partial |= amount > Decimal::ZERO,
        }
    }

    fn result(&self) -> (Option<Decimal>, Option<Decimal>, bool) {
        if self.months == 0 {
            return (None, None, self.partial);
        }
        let base = (self.base / Decimal::from(self.months)).round_dp(2);
        (Some(base), percent(self.numerator, base), self.partial)
    }
}
