//! O motor do aporte: dado quanto entra, o que comprar.
//!
//! O desenho e o porquê de cada passo estão em `docs/alocacao.md`. Em resumo:
//!
//! 1. `N` = patrimônio atual + aporte. A meta de cada classe é `meta% × N`, e o
//!    que falta nela (`gap`) é a meta menos o que já existe, com piso em zero.
//! 2. O aporte vai para as classes que podem recebê-lo (ao menos um ativo com
//!    nota e preço), na proporção do que falta em cada uma, **com teto no que
//!    falta** — nunca além da meta.
//! 3. Como `Σ metas = N`, a soma do que falta é sempre ≥ o aporte. Só sobra
//!    dinheiro depois de encher as classes elegíveis se alguma classe abaixo da
//!    meta não tiver para onde mandar — e essa sobra volta como `unallocated`,
//!    com o aviso dizendo qual classe precisa de um ativo.
//! 4. Dentro da classe, a meta de cada ativo é a fatia `nota / Σ notas` da
//!    classe depois do aporte, e o dinheiro vai na proporção do que falta a
//!    cada ativo.
//! 5. Cota e ação são inteiras (arredonda para baixo); cripto tem 8 casas;
//!    título do Tesouro Direto vai em centésimos de título; renda fixa por
//!    valor vai em centavos.
//! 6. O troco do arredondamento passa por uma rodada gulosa: compra mais uma
//!    unidade do ativo mais atrasado enquanto isso aproximar a carteira da meta
//!    (falta > meio preço); o que não couber em unidade vai para o ativo
//!    fracionável mais atrasado. O que sobra é `change`.

use rust_decimal::{Decimal, RoundingStrategy};
use uuid::Uuid;

use super::asset_class::{AssetClass, UnitKind};

/// 100% em pontos-base.
pub const FULL_BP: u32 = 10_000;

/// Teto de passos da rodada do troco. Cada passo gasta ao menos um centavo ou
/// uma unidade, então o teto só existe para o pior caso nunca virar laço longo.
const MAX_LEFTOVER_STEPS: usize = 20_000;

const CENT: Decimal = Decimal::from_parts(1, 0, 0, false, 2);
const HUNDRED: Decimal = Decimal::ONE_HUNDRED;

/// Um ativo como o motor o vê.
#[derive(Debug, Clone)]
pub struct Holding {
    pub asset_id: Uuid,
    pub asset_class: AssetClass,
    /// Símbolo ou nome, para os avisos.
    pub label: String,
    /// Valor atual em reais.
    pub value: Decimal,
    /// Preço de uma unidade em reais. Ativos por valor usam 1.
    pub unit_price: Option<Decimal>,
    /// Como se compra: vem do mercado do ativo, não da classe (um ETF de
    /// bitcoin da B3 é cota inteira dentro de Criptomoedas).
    pub unit: UnitKind,
    pub score: u8,
}

impl Holding {
    fn price(&self) -> Option<Decimal> {
        self.unit_price.filter(|p| *p > Decimal::ZERO)
    }

    fn is_eligible(&self) -> bool {
        self.score > 0 && self.price().is_some()
    }
}

/// Meta de cada classe em pontos-base, na ordem de [`AssetClass::ALL`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Targets(pub [u32; 7]);

impl Targets {
    pub fn get(&self, class: AssetClass) -> u32 {
        self.0[class_index(class)]
    }

    pub fn set(&mut self, class: AssetClass, bp: u32) {
        self.0[class_index(class)] = bp;
    }

    pub fn sum(&self) -> u32 {
        self.0.iter().sum()
    }
}

pub fn class_index(class: AssetClass) -> usize {
    class as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassStatus {
    Receiving,
    OnTarget,
    AboveTarget,
    BelowTarget,
    Blocked,
    NoTarget,
}

impl ClassStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ClassStatus::Receiving => "receiving",
            ClassStatus::OnTarget => "on_target",
            ClassStatus::AboveTarget => "above_target",
            ClassStatus::BelowTarget => "below_target",
            ClassStatus::Blocked => "blocked",
            ClassStatus::NoTarget => "no_target",
        }
    }
}

/// Por que uma classe abaixo da meta não recebe nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// Nenhum ativo cadastrado na classe.
    NoAssets,
    /// Há ativos, mas nenhum com nota.
    Unrated,
    /// Há ativo com nota, mas sem preço (cotação falhou ou símbolo errado).
    Unpriced,
}

impl BlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockReason::NoAssets => "no_assets",
            BlockReason::Unrated => "unrated",
            BlockReason::Unpriced => "unpriced",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// A classe está abaixo da meta e não tem ativo elegível; `amount` é quanto
    /// falta nela.
    ClassBlocked {
        asset_class: AssetClass,
        amount: Decimal,
        reason: BlockReason,
    },
    AssetsWithoutPrice {
        labels: Vec<String>,
    },
    AssetsWithoutScore {
        labels: Vec<String>,
    },
    NoTargets,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanItem {
    pub asset_id: Uuid,
    pub asset_class: AssetClass,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub value: Decimal,
    pub divisible: bool,
    /// Fatia do ativo na própria classe depois do aporte, de 0 a 100.
    pub class_percent_after: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassOutcome {
    pub asset_class: AssetClass,
    pub target_percent: Decimal,
    pub percent_before: Decimal,
    pub percent_after: Decimal,
    pub allocated: Decimal,
    pub status: ClassStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub amount: Decimal,
    pub invested: Decimal,
    pub change: Decimal,
    pub unallocated: Decimal,
    pub total_before: Decimal,
    pub total_after: Decimal,
    pub items: Vec<PlanItem>,
    pub classes: Vec<ClassOutcome>,
    pub notices: Vec<Notice>,
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

/// Calcula o plano de compra para `amount` reais.
pub fn plan(amount: Decimal, targets: Targets, holdings: &[Holding]) -> Plan {
    let amount = floor_cents(amount.max(Decimal::ZERO));
    let total: Decimal = holdings.iter().map(|h| h.value).sum();
    let grand_total = total + amount;
    let bp_sum = targets.sum();

    let mut classes = [ClassState::default(); 7];
    for holding in holdings {
        let state = &mut classes[class_index(holding.asset_class)];
        state.current += holding.value;
        if holding.is_eligible() {
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
            .filter(|&i| holdings[i].asset_class == class && holdings[i].is_eligible())
            .collect();
        let gaps: Vec<Decimal> = members
            .iter()
            .map(|&i| {
                let share = slice(eligible_after, holdings[i].score, state.score_sum);
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
        let Some(price) = holding.price() else {
            continue;
        };
        if desired[i] <= Decimal::ZERO || !holding.is_eligible() {
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
            if h.is_eligible() && state.score_sum > Decimal::ZERO {
                slice(
                    (state.target_value - state.ineligible_value).max(Decimal::ZERO),
                    h.score,
                    state.score_sum,
                )
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
            .filter(|&i| holdings[i].is_eligible() && holdings[i].unit == UnitKind::Whole)
            .filter_map(|i| holdings[i].price().map(|p| (i, p)))
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
            .filter(|&i| holdings[i].is_eligible() && holdings[i].unit != UnitKind::Whole)
            .filter(|&i| deficit(i) >= CENT && class_deficit(i) >= CENT)
            .max_by(|&a, &b| deficit(a).cmp(&deficit(b)));
        let Some(i) = fractional else { break };
        let Some(price) = holdings[i].price() else {
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
            let price = holding.price()?;
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
        .filter(|h| h.price().is_none())
        .map(|h| h.label.clone())
        .collect();
    if !without_price.is_empty() {
        out.push(Notice::AssetsWithoutPrice {
            labels: without_price,
        });
    }
    let without_score: Vec<String> = holdings
        .iter()
        .filter(|h| h.price().is_some() && h.score == 0)
        .map(|h| h.label.clone())
        .collect();
    if !without_score.is_empty() {
        out.push(Notice::AssetsWithoutScore {
            labels: without_score,
        });
    }
    out
}

/// Quanto dá para comprar com `budget` a `price` por unidade: (quantidade, valor).
fn purchasable(kind: UnitKind, budget: Decimal, price: Decimal) -> (Decimal, Decimal) {
    match kind {
        UnitKind::Whole => {
            let units = (budget / price).floor();
            (units, units * price)
        }
        UnitKind::Fractional => {
            let qty = (budget / price).round_dp_with_strategy(8, RoundingStrategy::ToZero);
            (qty, floor_cents(qty * price))
        }
        UnitKind::Hundredth => {
            let qty = (budget / price).round_dp_with_strategy(2, RoundingStrategy::ToZero);
            (qty, floor_cents(qty * price))
        }
        UnitKind::Value => {
            let value = floor_cents(budget);
            (value, value)
        }
    }
}

/// A fatia de `base` que cabe a um ativo de nota `score` numa classe cuja soma
/// das notas elegíveis é `score_sum` (passo 4 do motor: `nota / Σ notas`).
/// É a única fórmula da fatia: o plano e o % ideal da Carteira passam por ela.
fn slice(base: Decimal, score: u8, score_sum: Decimal) -> Decimal {
    base * Decimal::from(score) / score_sum
}

/// O % ideal de cada ativo dentro da própria classe: a fatia `nota / Σ notas`
/// do passo 4, em percentual de 0 a 100, na ordem de `holdings`.
///
/// Ativo sem nota ou sem preço fica em 0 (não recebe, então não tem fatia), e
/// numa classe com ao menos um ativo elegível a soma dos ideais é 100. Sem
/// elegível, todos ficam em 0. O valor sai sem arredondar; quem exibe arredonda.
pub fn ideal_class_percents(holdings: &[Holding]) -> Vec<Decimal> {
    let mut score_sums = [Decimal::ZERO; 7];
    for holding in holdings.iter().filter(|h| h.is_eligible()) {
        score_sums[class_index(holding.asset_class)] += Decimal::from(holding.score);
    }
    holdings
        .iter()
        .map(|holding| {
            let score_sum = score_sums[class_index(holding.asset_class)];
            if holding.is_eligible() && score_sum > Decimal::ZERO {
                slice(HUNDRED, holding.score, score_sum)
            } else {
                Decimal::ZERO
            }
        })
        .collect()
}

/// `part / whole` em percentual com duas casas; zero quando `whole` é zero.
pub fn percent(part: Decimal, whole: Decimal) -> Decimal {
    if whole <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    (part * HUNDRED / whole).round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

pub fn round_cents(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn floor_cents(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::ToZero)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::asset_class::Market;
    use rust_decimal_macros::dec;

    fn holding(
        class: AssetClass,
        label: &str,
        value: Decimal,
        price: Option<Decimal>,
        score: u8,
    ) -> Holding {
        Holding {
            asset_id: Uuid::new_v4(),
            asset_class: class,
            label: label.to_owned(),
            value,
            unit_price: price,
            unit: class.native_market().unit_kind(),
            score,
        }
    }

    /// O mesmo ativo, negociado na B3 (ETF ou BDR): cota inteira.
    fn on_b3(holding: Holding) -> Holding {
        Holding {
            unit: holding.asset_class.market(true).unit_kind(),
            ..holding
        }
    }

    /// Carteira fictícia de R$ 120.000,00: sete FIIs (tickers e preços de
    /// mercado, quantidades inventadas), um pouco de BTC e duas classes acima da
    /// meta, agregadas.
    fn sample_portfolio() -> Vec<Holding> {
        use AssetClass::*;
        vec![
            holding(BrReits, "KNRI11", dec!(1420.00), Some(dec!(142.00)), 6),
            holding(BrReits, "HGLG11", dec!(1689.60), Some(dec!(140.80)), 6),
            holding(BrReits, "VISC11", dec!(1898.60), Some(dec!(94.93)), 6),
            holding(BrReits, "XPML11", dec!(1256.25), Some(dec!(83.75)), 6),
            holding(BrReits, "BTLG11", dec!(756.40), Some(dec!(94.55)), 6),
            holding(BrReits, "TRXF11", dec!(1614.30), Some(dec!(53.81)), 6),
            holding(BrReits, "MXRF11", dec!(1255.50), Some(dec!(8.37)), 2),
            holding(Crypto, "BTC", dec!(103.50), Some(dec!(345000)), 8),
            holding(BrStocks, "ACOES", dec!(62000.00), Some(dec!(10.00)), 6),
            holding(FixedIncome, "RF", dec!(48005.85), Some(Decimal::ONE), 5),
        ]
    }

    /// 30/40/10/5/15: Ações Internacionais com meta e sem ativo nenhum.
    fn sample_targets() -> Targets {
        let mut t = Targets::default();
        t.set(AssetClass::IntlStocks, 3000);
        t.set(AssetClass::BrStocks, 4000);
        t.set(AssetClass::BrReits, 1000);
        t.set(AssetClass::Crypto, 500);
        t.set(AssetClass::FixedIncome, 1500);
        t
    }

    fn item<'a>(plan: &'a Plan, holdings: &[Holding], label: &str) -> Option<&'a PlanItem> {
        let id = holdings.iter().find(|h| h.label == label)?.asset_id;
        plan.items.iter().find(|i| i.asset_id == id)
    }

    fn class(plan: &Plan, class: AssetClass) -> &ClassOutcome {
        &plan.classes[class_index(class)]
    }

    fn assert_money_adds_up(plan: &Plan) {
        assert_eq!(
            plan.amount,
            plan.invested + plan.change + plan.unallocated,
            "{plan:#?}"
        );
        assert!(plan.change >= Decimal::ZERO && plan.unallocated >= Decimal::ZERO);
    }

    #[test]
    fn six_thousand_goes_to_crypto_and_the_lagging_reits_and_almost_nothing_is_left() {
        let holdings = sample_portfolio();
        let plan = plan(dec!(6000), sample_targets(), &holdings);
        assert_money_adds_up(&plan);

        // O troco do arredondamento passa pela rodada gulosa e o resto cai no
        // fracionável: sobra menos que alguns centavos.
        assert!(plan.change < dec!(0.05), "troco {}", plan.change);
        assert_eq!(plan.unallocated, Decimal::ZERO);

        let btc = item(&plan, &holdings, "BTC").expect("BTC recebe");
        assert!(
            btc.value > dec!(4100) && btc.value < dec!(4300),
            "BTC {}",
            btc.value
        );
        assert!(btc.divisible);
        let units = |label: &str| item(&plan, &holdings, label).map(|i| i.quantity);
        assert_eq!(units("BTLG11"), Some(dec!(10)));
        assert_eq!(units("XPML11"), Some(dec!(5)));
        assert_eq!(units("TRXF11"), Some(dec!(3)));
        assert_eq!(units("KNRI11"), Some(dec!(2)));
        // Quem está acima da própria fatia (ou não coube) não recebe.
        for label in ["VISC11", "MXRF11", "ACOES", "RF"] {
            assert!(
                item(&plan, &holdings, label).is_none(),
                "{label} nao deveria receber"
            );
        }
        assert_eq!(
            class(&plan, AssetClass::BrReits).status,
            ClassStatus::Receiving
        );
        assert_eq!(
            class(&plan, AssetClass::BrStocks).status,
            ClassStatus::AboveTarget
        );
        assert_eq!(
            class(&plan, AssetClass::IntlStocks).status,
            ClassStatus::Blocked
        );
        assert_eq!(
            class(&plan, AssetClass::UsReits).status,
            ClassStatus::NoTarget
        );
    }

    #[test]
    fn a_big_contribution_never_overshoots_and_reports_what_has_nowhere_to_go() {
        let holdings = sample_portfolio();
        let plan = plan(dec!(200000), sample_targets(), &holdings);
        assert_money_adds_up(&plan);

        // As duas classes elegíveis param na meta (com folga de uma cota), e o
        // que caberia em Ações Internacionais vira `unallocated` com aviso — em
        // vez de inflar quem pode receber ou sumir em silêncio.
        let n = dec!(320000);
        let reits_after = dec!(9890.65) + class(&plan, AssetClass::BrReits).allocated;
        assert!(
            (reits_after - n * dec!(0.10)).abs() <= dec!(142),
            "FIIs {reits_after}"
        );
        let crypto_after = dec!(103.50) + class(&plan, AssetClass::Crypto).allocated;
        assert!(
            (crypto_after - n * dec!(0.05)).abs() < dec!(1),
            "cripto {crypto_after}"
        );

        // Ações Nacionais também ficam abaixo da meta com R$ 200 mil e vão até ela.
        let stocks_after = dec!(62000) + class(&plan, AssetClass::BrStocks).allocated;
        assert_eq!(stocks_after, n * dec!(0.40));
        // Σ do que falta = aporte + Σ do que sobra: o que não tem destino é o
        // buraco de Internacionais (96.000) menos o excesso da Renda Fixa (5,85).
        assert_eq!(plan.unallocated, dec!(95994.15));
        assert!(plan.notices.iter().any(|n| matches!(
            n,
            Notice::ClassBlocked { asset_class: AssetClass::IntlStocks, amount, reason: BlockReason::NoAssets }
                if *amount == dec!(96000)
        )));
    }

    #[test]
    fn without_targets_nothing_is_bought() {
        let holdings = sample_portfolio();
        let plan = plan(dec!(1000), Targets::default(), &holdings);
        assert_money_adds_up(&plan);
        assert!(plan.items.is_empty());
        assert_eq!(plan.unallocated, dec!(1000));
        assert!(plan.notices.contains(&Notice::NoTargets));
        assert!(plan
            .classes
            .iter()
            .all(|c| c.status == ClassStatus::NoTarget));
    }

    #[test]
    fn a_small_contribution_still_buys_one_unit_of_the_most_behind_asset() {
        use AssetClass::*;
        let holdings = vec![
            holding(BrReits, "A11", dec!(1000), Some(dec!(100)), 5),
            holding(BrReits, "B11", dec!(400), Some(dec!(100)), 5),
        ];
        let mut targets = Targets::default();
        targets.set(BrReits, FULL_BP);
        let plan = plan(dec!(150), targets, &holdings);
        assert_money_adds_up(&plan);
        assert_eq!(
            item(&plan, &holdings, "B11").map(|i| i.quantity),
            Some(dec!(1))
        );
        assert_eq!(plan.change, dec!(50));
    }

    /// O mesmo ativo como título do Tesouro Direto: centésimos de título.
    fn as_treasury(holding: Holding) -> Holding {
        Holding {
            unit: Market::Treasury.unit_kind(),
            ..holding
        }
    }

    #[test]
    fn a_treasury_bond_is_bought_in_hundredths_of_a_bond() {
        use AssetClass::*;
        let holdings = vec![
            as_treasury(holding(
                FixedIncome,
                "IPCA",
                dec!(0),
                Some(dec!(4248.81)),
                5,
            )),
            holding(BrStocks, "ITSA4", dec!(1000), Some(dec!(10)), 5),
        ];
        let mut targets = Targets::default();
        targets.set(FixedIncome, 5000);
        targets.set(BrStocks, 5000);
        let plan = plan(dec!(1000), targets, &holdings);
        assert_money_adds_up(&plan);
        let bond = item(&plan, &holdings, "IPCA").expect("o titulo recebe");
        // R$ 1.000 a R$ 4.248,81 o título: 0,23 título (0,2353… para baixo),
        // R$ 977,22. O que não coube em centésimo de título volta como troco.
        assert_eq!(bond.quantity, dec!(0.23));
        assert_eq!(bond.value, dec!(977.22));
        assert!(bond.divisible);
        assert_eq!(plan.change, dec!(22.78));
    }

    /// A rodada do troco olhava só a fatia do ativo: um FII recém-cadastrado
    /// numa classe muito acima da meta recebia o troco inteiro, e a classe
    /// "acima da meta" aparecia recebendo. O troco agora também respeita o que
    /// falta na classe.
    #[test]
    fn change_never_goes_to_a_class_above_target() {
        use AssetClass::*;
        let holdings = vec![
            holding(BrReits, "CHEIO11", dec!(100000), Some(dec!(100)), 5),
            holding(BrReits, "NOVO11", Decimal::ZERO, Some(dec!(10)), 5),
            holding(BrStocks, "CARO3", Decimal::ZERO, Some(dec!(3000)), 5),
        ];
        let mut targets = Targets::default();
        targets.set(BrReits, 5000);
        targets.set(BrStocks, 5000);
        let plan = plan(dec!(1000), targets, &holdings);
        assert_money_adds_up(&plan);
        assert!(plan.items.is_empty(), "{:#?}", plan.items);
        assert_eq!(plan.change, dec!(1000));
        assert_eq!(class(&plan, BrReits).status, ClassStatus::AboveTarget);
        assert_eq!(class(&plan, BrStocks).status, ClassStatus::BelowTarget);
    }

    #[test]
    fn fixed_income_takes_cents_and_zero_score_takes_nothing() {
        use AssetClass::*;
        let holdings = vec![
            holding(FixedIncome, "Tesouro", dec!(1000), Some(Decimal::ONE), 5),
            holding(BrStocks, "ITSA4", dec!(1000), Some(dec!(10)), 0),
            holding(BrStocks, "SEMPRECO", dec!(0), None, 7),
        ];
        let mut targets = Targets::default();
        targets.set(FixedIncome, 5000);
        targets.set(BrStocks, 5000);
        let plan = plan(dec!(333.33), targets, &holdings);
        assert_money_adds_up(&plan);
        let rf = item(&plan, &holdings, "Tesouro").expect("renda fixa recebe");
        assert!(rf.divisible);
        assert_eq!(rf.value, rf.quantity);
        assert!(item(&plan, &holdings, "ITSA4").is_none());
        assert_eq!(class(&plan, BrStocks).status, ClassStatus::Blocked);
        assert!(plan.notices.contains(&Notice::AssetsWithoutPrice {
            labels: vec!["SEMPRECO".into()]
        }));
        assert!(plan.notices.contains(&Notice::AssetsWithoutScore {
            labels: vec!["ITSA4".into()]
        }));
    }

    #[test]
    fn zero_amount_is_a_diagnosis_and_negative_amount_is_zero() {
        let holdings = sample_portfolio();
        let plan = plan(dec!(-10), sample_targets(), &holdings);
        assert_money_adds_up(&plan);
        assert_eq!(plan.amount, Decimal::ZERO);
        assert!(plan.items.is_empty());
        assert_eq!(
            class(&plan, AssetClass::BrReits).status,
            ClassStatus::BelowTarget
        );
        assert_eq!(class(&plan, AssetClass::BrReits).percent_before, dec!(8.24));
    }

    #[test]
    fn an_empty_portfolio_with_targets_starts_from_scratch() {
        use AssetClass::*;
        let holdings = vec![
            holding(BrStocks, "WEGE3", Decimal::ZERO, Some(dec!(50)), 8),
            holding(Crypto, "BTC", Decimal::ZERO, Some(dec!(300000)), 5),
        ];
        let mut targets = Targets::default();
        targets.set(BrStocks, 8000);
        targets.set(Crypto, 2000);
        let plan = plan(dec!(1000), targets, &holdings);
        assert_money_adds_up(&plan);
        assert_eq!(
            item(&plan, &holdings, "WEGE3").map(|i| i.quantity),
            Some(dec!(16))
        );
        let btc = item(&plan, &holdings, "BTC").expect("BTC recebe");
        assert!(btc.value >= dec!(199) && btc.value <= dec!(200));
        assert_eq!(class(&plan, BrStocks).percent_before, Decimal::ZERO);
    }

    /// O cenário que motivou a lista de ativos sem posição: tudo em Ações
    /// Nacionais, acima da meta, e as outras classes vazias. Sem ativo nelas, o
    /// aporte inteiro fica sem destino; com ativos cadastrados e quantidade 0
    /// (só a nota), ele se divide pelas classes que faltam, e Ações Nacionais
    /// não recebe nada.
    #[test]
    fn assets_not_bought_yet_take_the_contribution_by_score() {
        use AssetClass::*;
        let mut targets = Targets::default();
        targets.set(BrStocks, 3500);
        targets.set(BrReits, 1000);
        targets.set(IntlStocks, 2500);
        targets.set(UsReits, 500);
        targets.set(Crypto, 500);
        targets.set(FixedIncome, 2000);
        let concentrated = vec![holding(BrStocks, "WEGE3", dec!(100000), Some(dec!(50)), 8)];

        let blocked = plan(dec!(5000), targets, &concentrated);
        assert!(blocked.items.is_empty());
        assert_eq!(blocked.unallocated, dec!(5000));
        assert!(blocked.notices.contains(&Notice::ClassBlocked {
            asset_class: Crypto,
            amount: dec!(5250),
            reason: BlockReason::NoAssets,
        }));

        let mut listed = concentrated;
        listed.extend([
            holding(BrReits, "KNRI11", Decimal::ZERO, Some(dec!(140)), 6),
            holding(BrReits, "MXRF11", Decimal::ZERO, Some(dec!(9)), 2),
            holding(IntlStocks, "VOO", Decimal::ZERO, Some(dec!(2900)), 7),
            on_b3(holding(
                IntlStocks,
                "IVVB11",
                Decimal::ZERO,
                Some(dec!(380)),
                7,
            )),
            holding(UsReits, "O", Decimal::ZERO, Some(dec!(310)), 5),
            on_b3(holding(Crypto, "QBTC11", Decimal::ZERO, Some(dec!(50)), 6)),
            holding(
                FixedIncome,
                "Tesouro Selic",
                Decimal::ZERO,
                Some(Decimal::ONE),
                5,
            ),
        ]);
        let plan = plan(dec!(5000), targets, &listed);
        assert_money_adds_up(&plan);
        assert_eq!(plan.unallocated, Decimal::ZERO);
        assert!(plan.change < dec!(50), "troco {}", plan.change);
        assert_eq!(class(&plan, BrStocks).allocated, Decimal::ZERO);
        assert_eq!(class(&plan, BrStocks).status, ClassStatus::AboveTarget);
        assert!(!plan
            .notices
            .iter()
            .any(|n| matches!(n, Notice::ClassBlocked { .. })));
        // O ETF de bitcoin da B3 sai em cota inteira, mesmo dentro de Criptomoedas.
        let etf = item(&plan, &listed, "QBTC11").expect("cripto recebe pelo ETF");
        assert!(!etf.divisible);
        assert_eq!(etf.quantity.fract(), Decimal::ZERO);
        // Dentro de FIIs, a nota 6 pesa três vezes a nota 2.
        let knri = item(&plan, &listed, "KNRI11").map_or(Decimal::ZERO, |i| i.value);
        let mxrf = item(&plan, &listed, "MXRF11").map_or(Decimal::ZERO, |i| i.value);
        assert!(knri > mxrf, "KNRI11 {knri} x MXRF11 {mxrf}");
    }

    #[test]
    fn a_blocked_class_says_what_is_missing() {
        use AssetClass::*;
        let mut targets = Targets::default();
        targets.set(BrReits, 2500);
        targets.set(IntlStocks, 2500);
        targets.set(Crypto, 2500);
        targets.set(UsReits, 2500);
        let holdings = vec![
            holding(BrReits, "SEMNOTA11", Decimal::ZERO, Some(dec!(100)), 0),
            holding(Crypto, "QBTC11", Decimal::ZERO, None, 6),
            holding(Crypto, "ZERO", Decimal::ZERO, Some(dec!(10)), 0),
        ];
        let plan = plan(dec!(1000), targets, &holdings);
        let reason = |class: AssetClass| {
            plan.notices.iter().find_map(|n| match n {
                Notice::ClassBlocked {
                    asset_class,
                    reason,
                    ..
                } if *asset_class == class => Some(*reason),
                _ => None,
            })
        };
        assert_eq!(reason(BrReits), Some(BlockReason::Unrated));
        assert_eq!(reason(Crypto), Some(BlockReason::Unpriced));
        assert_eq!(reason(IntlStocks), Some(BlockReason::NoAssets));
        assert_eq!(BlockReason::NoAssets.as_str(), "no_assets");
        assert_eq!(BlockReason::Unrated.as_str(), "unrated");
        assert_eq!(BlockReason::Unpriced.as_str(), "unpriced");
    }

    #[test]
    fn ideal_percent_is_the_score_slice_of_the_class() {
        use AssetClass::*;
        let holdings = vec![
            holding(BrReits, "A", dec!(1000), Some(dec!(10)), 6),
            holding(BrReits, "B", dec!(500), Some(dec!(10)), 6),
            holding(BrReits, "C", dec!(100), Some(dec!(10)), 2),
            holding(BrReits, "SEM-NOTA", dec!(300), Some(dec!(10)), 0),
            holding(BrReits, "SEM-PRECO", Decimal::ZERO, None, 8),
            holding(Crypto, "BTC", dec!(50), Some(dec!(345000)), 3),
        ];
        let ideal = ideal_class_percents(&holdings);
        // 6/14, 6/14, 2/14 da classe; quem não recebe fica em zero.
        assert_eq!(ideal[0].round_dp(2), dec!(42.86));
        assert_eq!(ideal[1].round_dp(2), dec!(42.86));
        assert_eq!(ideal[2].round_dp(2), dec!(14.29));
        assert_eq!(ideal[3], Decimal::ZERO);
        assert_eq!(ideal[4], Decimal::ZERO);
        assert_eq!(ideal[0] + ideal[1] + ideal[2], dec!(100));
        // Sozinho na classe, o ativo é 100%.
        assert_eq!(ideal[5], dec!(100));
    }

    #[test]
    fn ideal_percent_is_zero_when_nothing_in_the_class_can_receive() {
        use AssetClass::*;
        let holdings = vec![
            holding(BrStocks, "SEM-NOTA", dec!(900), Some(dec!(10)), 0),
            holding(BrStocks, "SEM-PRECO", Decimal::ZERO, None, 7),
        ];
        assert_eq!(
            ideal_class_percents(&holdings),
            vec![Decimal::ZERO, Decimal::ZERO]
        );
        assert!(ideal_class_percents(&[]).is_empty());
    }

    #[test]
    fn the_ideal_percent_is_the_slice_the_plan_aims_at() {
        // Duas classes com um ativo elegível e um de nota 0 que ocupa espaço:
        // com meta 100% numa classe só, o plano leva cada elegível até
        // `ideal% × (classe − ocupado)`, e o ideal exibido é esse mesmo %.
        use AssetClass::*;
        let holdings = vec![
            holding(BrReits, "A", dec!(0), Some(dec!(1)), 3),
            holding(BrReits, "B", dec!(0), Some(dec!(1)), 1),
            holding(BrReits, "ZERO", dec!(100), Some(dec!(1)), 0),
        ];
        let mut targets = Targets::default();
        targets.set(BrReits, FULL_BP);
        let plan = plan(dec!(300), targets, &holdings);
        let ideal = ideal_class_percents(&holdings);
        let spendable = dec!(300);
        for (i, label) in ["A", "B"].into_iter().enumerate() {
            let bought = item(&plan, &holdings, label).expect("compra").value;
            assert_eq!(
                bought,
                round_cents(spendable * ideal[i] / HUNDRED),
                "{label}"
            );
        }
    }

    #[test]
    fn percent_handles_zero_whole() {
        assert_eq!(percent(dec!(5), Decimal::ZERO), Decimal::ZERO);
        assert_eq!(percent(dec!(1), dec!(3)), dec!(33.33));
        assert_eq!(ClassStatus::Blocked.as_str(), "blocked");
        assert_eq!(ClassStatus::Receiving.as_str(), "receiving");
        assert_eq!(ClassStatus::OnTarget.as_str(), "on_target");
        assert_eq!(ClassStatus::AboveTarget.as_str(), "above_target");
        assert_eq!(ClassStatus::BelowTarget.as_str(), "below_target");
        assert_eq!(ClassStatus::NoTarget.as_str(), "no_target");
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        fn arb_holding() -> impl Strategy<Value = Holding> {
            (
                0usize..7,
                0u32..500_000,
                prop::option::weighted(0.9, 1u32..200_000),
                0u8..11,
                prop::bool::weighted(0.2),
                prop::bool::weighted(0.3),
            )
                .prop_map(
                    |(class, value_cents, price_cents, score, listed_on_b3, treasury)| {
                        let class = AssetClass::ALL[class];
                        // Título do Tesouro só existe em Renda Fixa (restrição do banco).
                        let treasury = treasury && class.accepts_treasury();
                        let unit = class
                            .market_of(listed_on_b3 && !treasury, treasury)
                            .unit_kind();
                        let price = match unit {
                            UnitKind::Value => Some(Decimal::ONE),
                            _ => price_cents.map(|c| Decimal::new(i64::from(c), 2)),
                        };
                        Holding {
                            unit,
                            ..holding(
                                class,
                                "X",
                                Decimal::new(i64::from(value_cents), 2),
                                price,
                                score,
                            )
                        }
                    },
                )
        }

        fn arb_targets() -> impl Strategy<Value = Targets> {
            prop::collection::vec(0u32..100, 7).prop_map(|weights| {
                let sum: u32 = weights.iter().sum();
                let mut t = Targets::default();
                if sum == 0 {
                    return t;
                }
                let mut assigned = 0;
                for (i, w) in weights.iter().enumerate() {
                    let bp = if i == 6 {
                        FULL_BP - assigned
                    } else {
                        w * FULL_BP / sum
                    };
                    t.0[i] = bp;
                    assigned += bp;
                }
                t
            })
        }

        /// Classe que já passou da meta não recebe nada; a que recebe para na
        /// meta, com folga de meia unidade por item (a rodada do troco só
        /// compra a unidade que aproxima da meta).
        fn assert_classes_never_pass_their_target(
            plan: &Plan,
            targets: Targets,
            holdings: &[Holding],
            amount: Decimal,
        ) -> Result<(), TestCaseError> {
            let total: Decimal = holdings.iter().map(|h| h.value).sum::<Decimal>() + amount;
            for class in AssetClass::ALL {
                let current: Decimal = holdings
                    .iter()
                    .filter(|h| h.asset_class == class)
                    .map(|h| h.value)
                    .sum();
                let target =
                    total * Decimal::from(targets.get(class)) / Decimal::from(targets.sum().max(1));
                let items: Vec<&PlanItem> = plan
                    .items
                    .iter()
                    .filter(|i| i.asset_class == class)
                    .collect();
                let added: Decimal = items.iter().map(|i| i.value).sum();
                if current >= target {
                    prop_assert!(items.is_empty(), "{class:?} acima da meta recebeu {added}");
                }
                let slack: Decimal = items
                    .iter()
                    .map(|i| i.unit_price / Decimal::TWO)
                    .sum::<Decimal>()
                    + CENT;
                prop_assert!(
                    added == Decimal::ZERO || current + added <= target + slack,
                    "{class:?} passou da meta: {} > {target}",
                    current + added
                );
            }
            Ok(())
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(300))]

            #[test]
            fn ideal_percents_sum_to_a_hundred_per_class_and_skip_ineligible(
                holdings in prop::collection::vec(arb_holding(), 0..12),
            ) {
                let ideal = ideal_class_percents(&holdings);
                prop_assert_eq!(ideal.len(), holdings.len());
                for class in AssetClass::ALL {
                    let members: Vec<usize> = (0..holdings.len())
                        .filter(|&i| holdings[i].asset_class == class)
                        .collect();
                    let sum: Decimal = members.iter().map(|&i| ideal[i]).sum();
                    if members.iter().any(|&i| holdings[i].is_eligible()) {
                        prop_assert!((sum - HUNDRED).abs() < dec!(0.000001), "soma {sum}");
                    } else {
                        prop_assert_eq!(sum, Decimal::ZERO);
                    }
                    for &i in &members {
                        prop_assert!(ideal[i] >= Decimal::ZERO && ideal[i] <= HUNDRED);
                        if !holdings[i].is_eligible() {
                            prop_assert_eq!(ideal[i], Decimal::ZERO);
                        }
                    }
                }
            }

            #[test]
            fn money_always_adds_up_and_nobody_overshoots(
                holdings in prop::collection::vec(arb_holding(), 0..12),
                targets in arb_targets(),
                amount_cents in 0u32..10_000_000,
            ) {
                let amount = Decimal::new(i64::from(amount_cents), 2);
                let plan = plan(amount, targets, &holdings);
                prop_assert_eq!(plan.amount, plan.invested + plan.change + plan.unallocated);
                prop_assert!(plan.change >= Decimal::ZERO);
                prop_assert!(plan.unallocated >= Decimal::ZERO);
                for item in &plan.items {
                    let h = holdings.iter().find(|h| h.asset_id == item.asset_id).expect("item de um ativo");
                    prop_assert!(h.score > 0, "nota zero recebeu");
                    prop_assert!(item.value > Decimal::ZERO);
                    // Nenhum ativo passa da própria meta por mais de meia unidade.
                    let total: Decimal = holdings.iter().map(|h| h.value).sum::<Decimal>() + amount;
                    let same_class = || holdings.iter().filter(|o| o.asset_class == h.asset_class);
                    let state_sum: Decimal = same_class().filter(|o| o.is_eligible()).map(|o| Decimal::from(o.score)).sum();
                    let ineligible: Decimal = same_class().filter(|o| !o.is_eligible()).map(|o| o.value).sum();
                    let bp = Decimal::from(targets.get(h.asset_class));
                    let class_target = total * bp / Decimal::from(targets.sum().max(1));
                    let ideal = (class_target - ineligible).max(Decimal::ZERO) * Decimal::from(h.score) / state_sum;
                    prop_assert!(
                        h.value + item.value <= ideal + item.unit_price / Decimal::TWO + CENT,
                        "{} passou da meta: {} > {}", h.label, h.value + item.value, ideal
                    );
                }
                assert_classes_never_pass_their_target(&plan, targets, &holdings, amount)?;
            }
        }
    }
}
