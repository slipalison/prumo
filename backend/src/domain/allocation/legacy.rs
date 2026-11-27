//! O motor como era antes do teto de preço e do limite de concentração,
//! congelado como oráculo de teste.
//!
//! O teste de propriedade `without_ceiling_or_limit_nothing_changes` compara o
//! motor de hoje, sem teto e sem limite, com esta cópia: as duas regras novas
//! não podem mudar um centavo de quem não as usa. Mudança intencional no motor
//! que altere o resultado sem teto nem limite atualiza esta cópia no mesmo
//! commit, com o porquê na mensagem.

use rust_decimal::Decimal;

use super::{
    class_index, floor_cents, percent, purchasable, round_cents, AssetClass, BlockReason,
    ClassOutcome, ClassStatus, Holding, Notice, Plan, PlanItem, Targets, UnitKind, CENT,
    MAX_LEFTOVER_STEPS,
};

fn price_of(h: &Holding) -> Option<Decimal> {
    h.unit_price.filter(|p| *p > Decimal::ZERO)
}

fn eligible_old(h: &Holding) -> bool {
    h.score > 0 && price_of(h).is_some()
}

/// Estado de uma classe durante o cálculo.
#[derive(Debug, Clone, Copy, Default)]
struct ClassState {
    current: Decimal,
    target_value: Decimal,
    gap: Decimal,
    eligible: bool,
    score_sum: Decimal,
    /// Valor dos ativos que não recebem (sem nota ou sem preço): ocupam espaço
    /// na classe, e a fatia dos elegíveis é o que sobra da meta.
    ineligible_value: Decimal,
    allocation: Decimal,
}

/// O plano como o motor calculava antes do teto e do limite.
pub(super) fn plan(amount: Decimal, targets: Targets, holdings: &[Holding]) -> Plan {
    let amount = floor_cents(amount.max(Decimal::ZERO));
    let total: Decimal = holdings.iter().map(|h| h.value).sum();
    let grand_total = total + amount;

    let mut classes = class_states(targets, holdings, grand_total);
    let unallocated = round_cents(split_between_classes(amount, &mut classes));
    let desired = desired_values(&classes, holdings);
    let (planned_qty, planned_value) = buyable(holdings, &desired);

    // Passo 6: o troco do arredondamento.
    let mut leftover = Leftover::new(
        holdings,
        &classes,
        ideal_values(&classes, holdings),
        planned_qty,
        planned_value,
    );
    let pool = amount - unallocated - leftover.planned_value.iter().copied().sum::<Decimal>();
    leftover.spend(pool);
    let Leftover {
        planned_qty,
        planned_value,
        ..
    } = leftover;

    let invested: Decimal = planned_value.iter().copied().sum();
    let change = amount - unallocated - invested;
    let total_after = total + invested;

    let mut class_added = [Decimal::ZERO; 7];
    for (i, holding) in holdings.iter().enumerate() {
        class_added[class_index(holding.asset_class)] += planned_value[i];
    }

    Plan {
        amount,
        invested: round_cents(invested),
        change: round_cents(change),
        unallocated,
        total_before: round_cents(total),
        total_after: round_cents(total_after),
        items: plan_items(
            holdings,
            &classes,
            &class_added,
            &planned_qty,
            &planned_value,
        ),
        classes: class_outcomes(targets, &classes, &class_added, total, total_after),
        notices: notices(targets, &classes, holdings),
    }
}

/// Passo 1: o que cada classe tem, a meta dela em `N` (`grand_total`) e o que
/// falta.
fn class_states(targets: Targets, holdings: &[Holding], grand_total: Decimal) -> [ClassState; 7] {
    let bp_sum = targets.sum();
    let mut classes = [ClassState::default(); 7];
    for holding in holdings {
        let state = &mut classes[class_index(holding.asset_class)];
        state.current += holding.value;
        if eligible_old(holding) {
            state.eligible = true;
            state.score_sum += Decimal::from(holding.score);
        } else {
            state.ineligible_value += holding.value;
        }
    }
    for class in AssetClass::ALL {
        let state = &mut classes[class_index(class)];
        state.target_value = if bp_sum == 0 {
            Decimal::ZERO
        } else {
            grand_total * Decimal::from(targets.get(class)) / Decimal::from(bp_sum)
        };
        state.gap = (state.target_value - state.current).max(Decimal::ZERO);
    }
    classes
}

/// Passo 2 e 3: entre classes, proporcional ao que falta, com teto no que
/// falta. Devolve o que ficou sem destino.
fn split_between_classes(amount: Decimal, classes: &mut [ClassState; 7]) -> Decimal {
    let receivable: Decimal = classes.iter().filter(|c| c.eligible).map(|c| c.gap).sum();
    if receivable <= Decimal::ZERO {
        return amount;
    }
    if amount <= receivable {
        for state in classes.iter_mut().filter(|c| c.eligible) {
            state.allocation = amount * state.gap / receivable;
        }
        return Decimal::ZERO;
    }
    for state in classes.iter_mut().filter(|c| c.eligible) {
        state.allocation = state.gap;
    }
    amount - receivable
}

/// Passo 4: dentro da classe, proporcional ao que falta a cada ativo.
fn desired_values(classes: &[ClassState; 7], holdings: &[Holding]) -> Vec<Decimal> {
    let mut desired = vec![Decimal::ZERO; holdings.len()];
    for class in AssetClass::ALL {
        let state = classes[class_index(class)];
        if state.allocation <= Decimal::ZERO || state.score_sum <= Decimal::ZERO {
            continue;
        }
        let eligible_after = state.current + state.allocation - state.ineligible_value;
        let members: Vec<usize> = (0..holdings.len())
            .filter(|&i| holdings[i].asset_class == class && eligible_old(&holdings[i]))
            .collect();
        let gaps: Vec<Decimal> = members
            .iter()
            .map(|&i| {
                let share = eligible_after * Decimal::from(holdings[i].score) / state.score_sum;
                (share - holdings[i].value).max(Decimal::ZERO)
            })
            .collect();
        let gap_sum: Decimal = gaps.iter().copied().sum();
        for (slot, &i) in members.iter().enumerate() {
            desired[i] = if gap_sum > Decimal::ZERO && state.allocation <= gap_sum {
                state.allocation * gaps[slot] / gap_sum
            } else {
                // Inalcançável quando Σ metas = N (ver o cabeçalho), mas se a
                // conta escorregar no arredondamento o excedente segue a nota.
                gaps[slot]
                    + (state.allocation - gap_sum).max(Decimal::ZERO)
                        * Decimal::from(holdings[i].score)
                        / state.score_sum
            };
        }
    }
    desired
}

/// Passo 5: converte em unidades compráveis (quantidade e valor de cada ativo).
fn buyable(holdings: &[Holding], desired: &[Decimal]) -> (Vec<Decimal>, Vec<Decimal>) {
    let mut planned_qty = vec![Decimal::ZERO; holdings.len()];
    let mut planned_value = vec![Decimal::ZERO; holdings.len()];
    for (i, holding) in holdings.iter().enumerate() {
        let Some(price) = price_of(holding) else {
            continue;
        };
        if desired[i] <= Decimal::ZERO || !eligible_old(holding) {
            continue;
        }
        let (qty, value) = purchasable(holding.unit, desired[i], price);
        planned_qty[i] = qty;
        planned_value[i] = value;
    }
    (planned_qty, planned_value)
}

/// A meta de cada ativo no troco: a fatia dele na meta da classe.
fn ideal_values(classes: &[ClassState; 7], holdings: &[Holding]) -> Vec<Decimal> {
    holdings
        .iter()
        .map(|h| {
            let state = classes[class_index(h.asset_class)];
            if eligible_old(h) && state.score_sum > Decimal::ZERO {
                (state.target_value - state.ineligible_value).max(Decimal::ZERO)
                    * Decimal::from(h.score)
                    / state.score_sum
            } else {
                Decimal::ZERO
            }
        })
        .collect()
}

/// Passo 6: a rodada gulosa do troco do arredondamento.
struct Leftover<'a> {
    holdings: &'a [Holding],
    classes: &'a [ClassState; 7],
    ideal: Vec<Decimal>,
    planned_qty: Vec<Decimal>,
    planned_value: Vec<Decimal>,
    class_planned: [Decimal; 7],
}

impl<'a> Leftover<'a> {
    fn new(
        holdings: &'a [Holding],
        classes: &'a [ClassState; 7],
        ideal: Vec<Decimal>,
        planned_qty: Vec<Decimal>,
        planned_value: Vec<Decimal>,
    ) -> Self {
        let mut class_planned = [Decimal::ZERO; 7];
        for (i, holding) in holdings.iter().enumerate() {
            class_planned[class_index(holding.asset_class)] += planned_value[i];
        }
        Self {
            holdings,
            classes,
            ideal,
            planned_qty,
            planned_value,
            class_planned,
        }
    }

    fn spend(&mut self, mut pool: Decimal) {
        for _ in 0..MAX_LEFTOVER_STEPS {
            if pool < CENT {
                break;
            }
            // Primeiro uma unidade inteira, se ela aproxima o ativo e a classe da meta.
            if let Some((i, price)) = self.next_whole(pool) {
                self.buy(i, Decimal::ONE, price);
                pool -= price;
                continue;
            }
            // Senão, o que cabe no fracionável mais atrasado.
            let Some(i) = self.next_fractional() else {
                break;
            };
            let Some(price) = price_of(&self.holdings[i]) else {
                break;
            };
            let budget = pool.min(self.deficit(i)).min(self.class_deficit(i));
            let (qty, value) = purchasable(self.holdings[i].unit, budget, price);
            if value <= Decimal::ZERO {
                break;
            }
            self.buy(i, qty, value);
            pool -= value;
        }
    }

    fn deficit(&self, i: usize) -> Decimal {
        self.ideal[i] - self.holdings[i].value - self.planned_value[i]
    }

    /// O que ainda falta na classe do ativo: o troco também não leva a
    /// classe além da meta (nem dá nada à classe que já passou dela).
    fn class_deficit(&self, i: usize) -> Decimal {
        let c = class_index(self.holdings[i].asset_class);
        self.classes[c].target_value - self.classes[c].current - self.class_planned[c]
    }

    fn next_whole(&self, pool: Decimal) -> Option<(usize, Decimal)> {
        let holdings = self.holdings;
        (0..holdings.len())
            .filter(|&i| eligible_old(&holdings[i]) && holdings[i].unit == UnitKind::Whole)
            .filter_map(|i| price_of(&holdings[i]).map(|p| (i, p)))
            .filter(|&(i, price)| {
                price <= pool
                    && self.deficit(i) * Decimal::TWO > price
                    && self.class_deficit(i) * Decimal::TWO > price
            })
            .max_by(|a, b| self.deficit(a.0).cmp(&self.deficit(b.0)))
    }

    fn next_fractional(&self) -> Option<usize> {
        let holdings = self.holdings;
        (0..holdings.len())
            .filter(|&i| eligible_old(&holdings[i]) && holdings[i].unit != UnitKind::Whole)
            .filter(|&i| self.deficit(i) >= CENT && self.class_deficit(i) >= CENT)
            .max_by(|&a, &b| self.deficit(a).cmp(&self.deficit(b)))
    }

    fn buy(&mut self, i: usize, quantity: Decimal, value: Decimal) {
        self.planned_qty[i] += quantity;
        self.planned_value[i] += value;
        self.class_planned[class_index(self.holdings[i].asset_class)] += value;
    }
}

fn plan_items(
    holdings: &[Holding],
    classes: &[ClassState; 7],
    class_added: &[Decimal; 7],
    planned_qty: &[Decimal],
    planned_value: &[Decimal],
) -> Vec<PlanItem> {
    holdings
        .iter()
        .enumerate()
        .filter(|(i, _)| planned_value[*i] > Decimal::ZERO)
        .filter_map(|(i, holding)| {
            let price = price_of(holding)?;
            let state = classes[class_index(holding.asset_class)];
            let class_after = state.current + class_added[class_index(holding.asset_class)];
            Some(PlanItem {
                asset_id: holding.asset_id,
                asset_class: holding.asset_class,
                quantity: planned_qty[i].normalize(),
                unit_price: price,
                value: round_cents(planned_value[i]),
                divisible: holding.unit != UnitKind::Whole,
                class_percent_after: percent(holding.value + planned_value[i], class_after),
            })
        })
        .collect()
}

fn class_outcomes(
    targets: Targets,
    classes: &[ClassState; 7],
    class_added: &[Decimal; 7],
    total: Decimal,
    total_after: Decimal,
) -> Vec<ClassOutcome> {
    let bp_sum = targets.sum();
    AssetClass::ALL
        .into_iter()
        .map(|class| {
            let state = classes[class_index(class)];
            let added = class_added[class_index(class)];
            ClassOutcome {
                asset_class: class,
                target_percent: percent(
                    Decimal::from(targets.get(class)),
                    Decimal::from(bp_sum.max(1)),
                ),
                percent_before: percent(state.current, total),
                percent_after: percent(state.current + added, total_after),
                allocated: round_cents(added),
                status: class_status(targets.get(class), added, &state),
            }
        })
        .collect()
}

fn class_status(target_bp: u32, added: Decimal, state: &ClassState) -> ClassStatus {
    if target_bp == 0 {
        ClassStatus::NoTarget
    } else if added > Decimal::ZERO {
        ClassStatus::Receiving
    } else if state.gap >= CENT && !state.eligible {
        ClassStatus::Blocked
    } else if state.gap >= CENT {
        ClassStatus::BelowTarget
    } else if state.current > state.target_value + CENT {
        ClassStatus::AboveTarget
    } else {
        ClassStatus::OnTarget
    }
}

fn notices(targets: Targets, classes: &[ClassState; 7], holdings: &[Holding]) -> Vec<Notice> {
    let mut out = Vec::new();
    if targets.sum() == 0 {
        out.push(Notice::NoTargets);
    }
    for class in AssetClass::ALL {
        let state = classes[class_index(class)];
        if targets.get(class) > 0 && !state.eligible && state.gap >= CENT {
            let mut members = holdings
                .iter()
                .filter(|h| h.asset_class == class)
                .peekable();
            let reason = if members.peek().is_none() {
                BlockReason::NoAssets
            } else if members.any(|h| h.score > 0) {
                // Tem nota e a classe segue bloqueada: falta o preço.
                BlockReason::Unpriced
            } else {
                BlockReason::Unrated
            };
            out.push(Notice::ClassBlocked {
                asset_class: class,
                amount: round_cents(state.gap),
                reason,
            });
        }
    }
    let without_price: Vec<String> = holdings
        .iter()
        .filter(|h| price_of(h).is_none())
        .map(|h| h.label.clone())
        .collect();
    if !without_price.is_empty() {
        out.push(Notice::AssetsWithoutPrice {
            labels: without_price,
        });
    }
    let without_score: Vec<String> = holdings
        .iter()
        .filter(|h| price_of(h).is_some() && h.score == 0)
        .map(|h| h.label.clone())
        .collect();
    if !without_score.is_empty() {
        out.push(Notice::AssetsWithoutScore {
            labels: without_score,
        });
    }
    out
}
