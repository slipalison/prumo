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

    // Passo 2 e 3: entre classes, proporcional ao que falta, com teto no que falta.
    let receivable: Decimal = classes.iter().filter(|c| c.eligible).map(|c| c.gap).sum();
    let mut unallocated = Decimal::ZERO;
    if receivable > Decimal::ZERO {
        if amount <= receivable {
            for state in classes.iter_mut().filter(|c| c.eligible) {
                state.allocation = amount * state.gap / receivable;
            }
        } else {
            for state in classes.iter_mut().filter(|c| c.eligible) {
                state.allocation = state.gap;
            }
            unallocated = amount - receivable;
        }
    } else {
        unallocated = amount;
    }
    unallocated = round_cents(unallocated);

    // Passo 4: dentro da classe, proporcional ao que falta a cada ativo.
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

    // Passo 5: converte em unidades compráveis.
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

    // Passo 6: o troco do arredondamento.
    let ideal: Vec<Decimal> = holdings
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
        .collect();
    let mut pool = amount - unallocated - planned_value.iter().copied().sum::<Decimal>();
    let mut class_planned = [Decimal::ZERO; 7];
    for (i, holding) in holdings.iter().enumerate() {
        class_planned[class_index(holding.asset_class)] += planned_value[i];
    }
    for _ in 0..MAX_LEFTOVER_STEPS {
        if pool < CENT {
            break;
        }
        let deficit = |i: usize| ideal[i] - holdings[i].value - planned_value[i];
        // O que ainda falta na classe do ativo: o troco também não leva a
        // classe além da meta (nem dá nada à classe que já passou dela).
        let class_deficit = |i: usize| {
            let c = class_index(holdings[i].asset_class);
            classes[c].target_value - classes[c].current - class_planned[c]
        };
        // Primeiro uma unidade inteira, se ela aproxima o ativo e a classe da meta.
        let whole = (0..holdings.len())
            .filter(|&i| eligible_old(&holdings[i]) && holdings[i].unit == UnitKind::Whole)
            .filter_map(|i| price_of(&holdings[i]).map(|p| (i, p)))
            .filter(|&(i, price)| {
                price <= pool
                    && deficit(i) * Decimal::TWO > price
                    && class_deficit(i) * Decimal::TWO > price
            })
            .max_by(|a, b| deficit(a.0).cmp(&deficit(b.0)));
        if let Some((i, price)) = whole {
            planned_qty[i] += Decimal::ONE;
            planned_value[i] += price;
            class_planned[class_index(holdings[i].asset_class)] += price;
            pool -= price;
            continue;
        }
        // Senão, o que cabe no fracionável mais atrasado.
        let fractional = (0..holdings.len())
            .filter(|&i| eligible_old(&holdings[i]) && holdings[i].unit != UnitKind::Whole)
            .filter(|&i| deficit(i) >= CENT && class_deficit(i) >= CENT)
            .max_by(|&a, &b| deficit(a).cmp(&deficit(b)));
        let Some(i) = fractional else { break };
        let Some(price) = price_of(&holdings[i]) else {
            break;
        };
        let budget = pool.min(deficit(i)).min(class_deficit(i));
        let (qty, value) = purchasable(holdings[i].unit, budget, price);
        if value <= Decimal::ZERO {
            break;
        }
        planned_qty[i] += qty;
        planned_value[i] += value;
        class_planned[class_index(holdings[i].asset_class)] += value;
        pool -= value;
    }

    let invested: Decimal = planned_value.iter().copied().sum();
    let change = amount - unallocated - invested;
    let total_after = total + invested;

    let mut class_added = [Decimal::ZERO; 7];
    for (i, holding) in holdings.iter().enumerate() {
        class_added[class_index(holding.asset_class)] += planned_value[i];
    }

    let items = holdings
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
        .collect();

    let outcomes = AssetClass::ALL
        .into_iter()
        .map(|class| {
            let state = classes[class_index(class)];
            let added = class_added[class_index(class)];
            let status = if targets.get(class) == 0 {
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
            };
            ClassOutcome {
                asset_class: class,
                target_percent: percent(
                    Decimal::from(targets.get(class)),
                    Decimal::from(bp_sum.max(1)),
                ),
                percent_before: percent(state.current, total),
                percent_after: percent(state.current + added, total_after),
                allocated: round_cents(added),
                status,
            }
        })
        .collect();

    Plan {
        amount,
        invested: round_cents(invested),
        change: round_cents(change),
        unallocated,
        total_before: round_cents(total),
        total_after: round_cents(total_after),
        items,
        classes: outcomes,
        notices: notices(targets, &classes, holdings),
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
