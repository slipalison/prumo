//! O motor do aporte: dado quanto entra, o que comprar.
//!
//! O desenho e o porquê de cada passo estão em `docs/alocacao.md`. Em resumo:
//!
//! 1. `N` = patrimônio atual + aporte. A meta de cada classe é `meta% × N`, e o
//!    que falta nela (`gap`) é a meta menos o que já existe, com piso em zero.
//! 2. O aporte vai para as classes que podem recebê-lo (ao menos um ativo com
//!    nota, preço e preço até o teto da pessoa), na proporção do que falta em
//!    cada uma, **com teto no que falta** — nunca além da meta. Com o limite
//!    de concentração ligado, a classe também não recebe mais do que cabe nos
//!    ativos dela sem passar do limite; o que não coube vai para as outras
//!    classes que ainda estão abaixo da meta.
//! 3. Como `Σ metas = N`, a soma do que falta é sempre ≥ o aporte. Só sobra
//!    dinheiro depois de encher as classes elegíveis se alguma classe abaixo da
//!    meta não tiver para onde mandar — e essa sobra volta como `unallocated`,
//!    com o aviso dizendo o motivo.
//! 4. Dentro da classe, a meta de cada ativo é a fatia `nota / Σ notas` da
//!    classe depois do aporte, e o dinheiro vai na proporção do que falta a
//!    cada ativo. Com o limite, quem bate nele fica nele e o resto da classe é
//!    redividido pela nota entre os outros.
//! 5. Cota e ação são inteiras (arredonda para baixo); cripto tem 8 casas;
//!    título do Tesouro Direto vai em centésimos de título; renda fixa por
//!    valor vai em centavos.
//! 6. O troco do arredondamento passa por uma rodada gulosa: compra mais uma
//!    unidade do ativo mais atrasado enquanto isso aproximar o ativo e a classe
//!    da meta (falta > meio preço) sem passar do limite; o que não couber em
//!    unidade vai para o ativo fracionável mais atrasado. O que sobra é
//!    `change`.
//!
//! Preço médio, rentabilidade e "caiu, compra mais" não entram em nenhum passo.

use rust_decimal::{Decimal, RoundingStrategy};
use uuid::Uuid;

use super::analysis::references::ReferenceGap;
use super::asset_class::{AssetClass, UnitKind};
use super::ceiling::{Ceiling, CeilingMethod, CeilingSource};

#[cfg(test)]
mod legacy;

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
    /// O teto de preço de hoje, pela escolha da pessoa (`domain::ceiling`).
    /// Acima dele, ou com o método escolhido sem valor, o ativo fica fora deste
    /// aporte.
    pub ceiling: Ceiling,
}

impl Holding {
    fn price(&self) -> Option<Decimal> {
        self.unit_price.filter(|p| *p > Decimal::ZERO)
    }

    /// O teto em reais por unidade, quando há um com valor.
    fn max_price(&self) -> Option<Decimal> {
        self.ceiling.value()
    }

    /// Tem nota e preço, mas o preço passou do teto da pessoa.
    fn above_ceiling(&self) -> bool {
        matches!((self.price(), self.max_price()), (Some(price), Some(max)) if price > max)
    }

    /// A pessoa escolheu um método de teto que hoje está sem valor.
    fn ceiling_unavailable(&self) -> Option<(CeilingMethod, ReferenceGap)> {
        match self.ceiling {
            Ceiling::Unavailable { method, reason } => Some((method, reason)),
            _ => None,
        }
    }

    /// O teto tira o ativo deste aporte: o preço passou dele, ou o método
    /// escolhido está sem valor.
    fn blocked_by_ceiling(&self) -> bool {
        self.above_ceiling() || self.ceiling_unavailable().is_some()
    }

    /// Tem fatia na composição-alvo: nota e preço. O teto não tira a fatia.
    fn has_slice(&self) -> bool {
        self.score > 0 && self.price().is_some()
    }

    /// Recebe neste aporte: tem fatia e o teto não o tira.
    fn is_eligible(&self) -> bool {
        self.has_slice() && !self.blocked_by_ceiling()
    }
}

/// Os guarda-corpos que a pessoa liga. O padrão é tudo desligado, e desligado
/// o motor dá exatamente o resultado de sempre.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// Nenhum ativo passa desta fatia de `N` (a carteira depois do aporte), em
    /// pontos-base.
    pub max_asset_bp: Option<u32>,
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
    /// Os ativos com nota e preço estão todos acima do teto de preço da pessoa.
    AboveCeiling,
    /// Os ativos com nota e preço estão fora pelo teto, e ao menos um porque o
    /// método de teto que a pessoa escolheu está sem valor hoje.
    CeilingUnavailable,
    /// Os ativos que receberiam já estão no limite de concentração.
    ConcentrationLimit,
}

impl BlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockReason::NoAssets => "no_assets",
            BlockReason::Unrated => "unrated",
            BlockReason::Unpriced => "unpriced",
            BlockReason::AboveCeiling => "above_ceiling",
            BlockReason::CeilingUnavailable => "ceiling_unavailable",
            BlockReason::ConcentrationLimit => "concentration_limit",
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
    /// O preço passou do teto da pessoa: o ativo fica fora deste aporte.
    AssetAboveCeiling {
        asset_id: Uuid,
        label: String,
        price: Decimal,
        max_price: Decimal,
        /// Valor próprio ou o método que deu o teto.
        source: CeilingSource,
    },
    /// O método de teto que a pessoa escolheu está sem valor hoje: o ativo fica
    /// fora deste aporte até voltar a ter (nunca cai para "sem teto").
    AssetCeilingUnavailable {
        asset_id: Uuid,
        label: String,
        method: CeilingMethod,
        reason: ReferenceGap,
    },
    /// O limite de concentração mudou o plano: `assets` pararam nele (o que
    /// iria para eles foi para os outros da classe), e `amount` ficou sem
    /// destino por causa dele.
    ConcentrationLimit {
        max_percent: Decimal,
        assets: Vec<(Uuid, String)>,
        amount: Decimal,
    },
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
    /// Tem ativo com nota, preço e preço até o teto.
    candidates: bool,
    /// Pode receber neste aporte: tem candidatos e, com o limite ligado,
    /// espaço neles.
    eligible: bool,
    score_sum: Decimal,
    /// Valor dos ativos que não recebem (sem nota, sem preço ou acima do teto):
    /// ocupam espaço na classe, e a fatia dos elegíveis é o que sobra da meta.
    ineligible_value: Decimal,
    /// Com o limite ligado: quanto cabe nos candidatos sem nenhum passar dele.
    capacity: Option<Decimal>,
    allocation: Decimal,
}

impl ClassState {
    /// O máximo que a classe recebe: o que falta nela e, com o limite, o que
    /// cabe nos ativos dela.
    fn ceiling(&self) -> Decimal {
        self.capacity.map_or(self.gap, |c| c.min(self.gap))
    }

    /// Soma um ativo da classe: o valor dele e, se recebe, a nota e o que cabe
    /// nele sob o limite.
    fn add(&mut self, holding: &Holding, asset_cap: Option<Decimal>) {
        self.current += holding.value;
        if holding.is_eligible() {
            self.candidates = true;
            self.score_sum += Decimal::from(holding.score);
            if let Some(cap) = asset_cap {
                *self.capacity.get_or_insert(Decimal::ZERO) +=
                    (cap - holding.value).max(Decimal::ZERO);
            }
        } else {
            self.ineligible_value += holding.value;
        }
    }
}

/// Calcula o plano de compra para `amount` reais.
pub fn plan(amount: Decimal, targets: Targets, holdings: &[Holding], limits: Limits) -> Plan {
    let amount = floor_cents(amount.max(Decimal::ZERO));
    let total: Decimal = holdings.iter().map(|h| h.value).sum();
    let grand_total = total + amount;
    // O limite de concentração em reais: a fatia máxima de um ativo em `N`.
    let asset_cap = limits
        .max_asset_bp
        .map(|bp| grand_total * Decimal::from(bp) / Decimal::from(FULL_BP));

    let mut classes = class_states(targets, holdings, grand_total, asset_cap);
    let unallocated = round_cents(split_between_classes(amount, &mut classes).max(Decimal::ZERO));
    let (desired, at_limit) = desired_values(&classes, holdings, asset_cap);
    let (planned_qty, planned_value) = buyable(holdings, &desired);

    // Passo 6: o troco do arredondamento.
    let mut leftover = Leftover::new(
        holdings,
        &classes,
        ideal_values(&classes, holdings, asset_cap),
        asset_cap,
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
    let items = plan_items(
        holdings,
        &classes,
        &class_added,
        &planned_qty,
        &planned_value,
    );
    let outcomes = class_outcomes(targets, &classes, &class_added, total, total_after);

    let mut notices = notices(targets, &classes, holdings);
    if let Some(bp) = limits.max_asset_bp {
        notices.extend(limit_notice(bp, &classes, holdings, &at_limit, unallocated));
    }

    Plan {
        amount,
        invested: round_cents(invested),
        change: round_cents(change),
        unallocated,
        total_before: round_cents(total),
        total_after: round_cents(total_after),
        items,
        classes: outcomes,
        notices,
    }
}

/// Até onde a fatia de um ativo pode ir: o limite, ou o que ele já vale (o
/// motor não vende quem já passou). `None` sem limite.
fn share_cap(asset_cap: Option<Decimal>, holding: &Holding) -> Option<Decimal> {
    asset_cap.map(|cap| cap.max(holding.value))
}

/// Passo 1: o que cada classe tem, a meta dela em `N` (`grand_total`), o que
/// falta e se ela pode receber.
fn class_states(
    targets: Targets,
    holdings: &[Holding],
    grand_total: Decimal,
    asset_cap: Option<Decimal>,
) -> [ClassState; 7] {
    let bp_sum = targets.sum();
    let mut classes = [ClassState::default(); 7];
    for holding in holdings {
        classes[class_index(holding.asset_class)].add(holding, asset_cap);
    }
    for class in AssetClass::ALL {
        let state = &mut classes[class_index(class)];
        state.target_value = if bp_sum == 0 {
            Decimal::ZERO
        } else {
            grand_total * Decimal::from(targets.get(class)) / Decimal::from(bp_sum)
        };
        state.gap = (state.target_value - state.current).max(Decimal::ZERO);
        state.eligible = state.candidates && state.capacity.is_none_or(|c| c >= CENT);
    }
    classes
}

/// Passos 2 e 3: entre classes, proporcional ao que falta, com teto no que
/// falta (e no que cabe sob o limite). A classe que bate no que cabe sai da
/// rodada, e o resto é redividido entre as outras pelo que ainda falta.
/// Devolve o que ficou sem destino.
fn split_between_classes(amount: Decimal, classes: &mut [ClassState; 7]) -> Decimal {
    let mut pool = amount;
    for _ in 0..=AssetClass::ALL.len() {
        let open: Vec<usize> = (0..classes.len())
            .filter(|&c| classes[c].eligible && classes[c].ceiling() > classes[c].allocation)
            .collect();
        let receivable: Decimal = open
            .iter()
            .map(|&c| classes[c].gap - classes[c].allocation)
            .sum();
        if receivable <= Decimal::ZERO || pool <= Decimal::ZERO {
            break;
        }
        let fits = pool <= receivable;
        let (spent, capped) = give_round(classes, &open, pool, receivable);
        if !capped {
            pool = if fits {
                Decimal::ZERO
            } else {
                pool - receivable
            };
            break;
        }
        pool -= spent;
    }
    pool
}

/// Uma rodada da divisão entre as classes `open`: cada uma recebe a parte dela
/// de `pool` (ou tudo o que falta nela, quando `pool` passa de `receivable`),
/// sem passar do que cabe sob o limite. Devolve quanto saiu e se alguma classe
/// parou no que cabe.
fn give_round(
    classes: &mut [ClassState; 7],
    open: &[usize],
    pool: Decimal,
    receivable: Decimal,
) -> (Decimal, bool) {
    let fits = pool <= receivable;
    let mut spent = Decimal::ZERO;
    let mut capped = false;
    for &c in open {
        let state = &mut classes[c];
        let missing = state.gap - state.allocation;
        let share = if fits {
            pool * missing / receivable
        } else {
            missing
        };
        let room = state.ceiling() - state.allocation;
        // Sem limite o teto é o que falta, e a divisão já não passa dele.
        let give = if state.capacity.is_some() && share > room {
            capped = true;
            room
        } else {
            share
        };
        state.allocation += give;
        spent += give;
    }
    (spent, capped)
}

/// Passo 4: dentro da classe, proporcional ao que falta a cada ativo. Devolve
/// quanto cada ativo deveria receber (na ordem de `holdings`) e quem parou no
/// limite.
fn desired_values(
    classes: &[ClassState; 7],
    holdings: &[Holding],
    asset_cap: Option<Decimal>,
) -> (Vec<Decimal>, Vec<usize>) {
    let mut desired = vec![Decimal::ZERO; holdings.len()];
    let mut at_limit: Vec<usize> = Vec::new();
    for class in AssetClass::ALL {
        let state = classes[class_index(class)];
        if state.allocation <= Decimal::ZERO || state.score_sum <= Decimal::ZERO {
            continue;
        }
        let eligible_after = state.current + state.allocation - state.ineligible_value;
        let members = eligible_members(holdings, class);
        let shares = capped_shares(eligible_after, state.score_sum, &members, holdings, |h| {
            share_cap(asset_cap, h)
        });
        let gaps: Vec<Decimal> = members
            .iter()
            .zip(&shares)
            .map(|(&i, share)| (*share - holdings[i].value).max(Decimal::ZERO))
            .collect();
        let gap_sum: Decimal = gaps.iter().copied().sum();
        for (slot, &i) in members.iter().enumerate() {
            desired[i] = if gap_sum > Decimal::ZERO && state.allocation <= gap_sum {
                state.allocation * gaps[slot] / gap_sum
            } else {
                // Inalcançável quando Σ metas = N (ver o cabeçalho), mas se a
                // conta escorregar no arredondamento o excedente segue a nota.
                spread_by_score(&state, gaps[slot], gap_sum, &holdings[i], asset_cap)
            };
            if share_cap(asset_cap, &holdings[i]).is_some_and(|cap| shares[slot] >= cap) {
                at_limit.push(i);
            }
        }
    }
    (desired, at_limit)
}

/// O que falta ao ativo mais a parte dele, pela nota, do que a classe recebe
/// além do que falta aos ativos dela; com o limite, até o que cabe nele.
fn spread_by_score(
    state: &ClassState,
    gap: Decimal,
    gap_sum: Decimal,
    holding: &Holding,
    asset_cap: Option<Decimal>,
) -> Decimal {
    let spread = gap
        + (state.allocation - gap_sum).max(Decimal::ZERO) * Decimal::from(holding.score)
            / state.score_sum;
    match asset_cap {
        Some(cap) => spread.min((cap - holding.value).max(Decimal::ZERO)),
        None => spread,
    }
}

/// Passo 5: converte em unidades compráveis. Devolve a quantidade e o valor de
/// cada ativo, na ordem de `holdings`.
fn buyable(holdings: &[Holding], desired: &[Decimal]) -> (Vec<Decimal>, Vec<Decimal>) {
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
    (planned_qty, planned_value)
}

/// A meta de cada ativo no troco: a fatia dele na meta da classe, com o mesmo
/// limite do passo 4.
fn ideal_values(
    classes: &[ClassState; 7],
    holdings: &[Holding],
    asset_cap: Option<Decimal>,
) -> Vec<Decimal> {
    let mut ideal = vec![Decimal::ZERO; holdings.len()];
    for class in AssetClass::ALL {
        let state = classes[class_index(class)];
        if state.score_sum <= Decimal::ZERO {
            continue;
        }
        let members = eligible_members(holdings, class);
        let base = (state.target_value - state.ineligible_value).max(Decimal::ZERO);
        let shares = capped_shares(base, state.score_sum, &members, holdings, |h| {
            share_cap(asset_cap, h)
        });
        for (&i, share) in members.iter().zip(shares) {
            ideal[i] = share;
        }
    }
    ideal
}

/// Passo 6: a rodada gulosa do troco do arredondamento, sobre o que os passos
/// anteriores já planejaram.
struct Leftover<'a> {
    holdings: &'a [Holding],
    classes: &'a [ClassState; 7],
    /// A meta de cada ativo ([`ideal_values`]).
    ideal: Vec<Decimal>,
    asset_cap: Option<Decimal>,
    planned_qty: Vec<Decimal>,
    planned_value: Vec<Decimal>,
    /// O valor planejado em cada classe.
    class_planned: [Decimal; 7],
}

impl<'a> Leftover<'a> {
    fn new(
        holdings: &'a [Holding],
        classes: &'a [ClassState; 7],
        ideal: Vec<Decimal>,
        asset_cap: Option<Decimal>,
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
            asset_cap,
            planned_qty,
            planned_value,
            class_planned,
        }
    }

    /// Gasta o troco `pool`, um passo por vez, enquanto houver centavo e passo
    /// que aproxime alguém da meta.
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
            let Some(price) = self.holdings[i].price() else {
                break;
            };
            let mut budget = pool.min(self.deficit(i)).min(self.class_deficit(i));
            if let Some(room) = self.limit_room(i) {
                budget = budget.min(room);
            }
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

    /// O que ainda falta na classe do ativo: o troco também não leva a classe
    /// além da meta (nem dá nada à classe que já passou dela).
    fn class_deficit(&self, i: usize) -> Decimal {
        let c = class_index(self.holdings[i].asset_class);
        self.classes[c].target_value - self.classes[c].current - self.class_planned[c]
    }

    /// O que ainda cabe no ativo sob o limite. Aqui não há meia unidade de
    /// folga: o limite é teto duro.
    fn limit_room(&self, i: usize) -> Option<Decimal> {
        self.asset_cap
            .map(|cap| cap - self.holdings[i].value - self.planned_value[i])
    }

    /// O ativo de unidade inteira mais atrasado cuja próxima unidade cabe no
    /// troco e aproxima o ativo e a classe da meta (falta > meio preço), sem
    /// passar do limite; com o preço dela.
    fn next_whole(&self, pool: Decimal) -> Option<(usize, Decimal)> {
        let holdings = self.holdings;
        (0..holdings.len())
            .filter(|&i| holdings[i].is_eligible() && holdings[i].unit == UnitKind::Whole)
            .filter_map(|i| holdings[i].price().map(|p| (i, p)))
            .filter(|&(i, price)| {
                price <= pool
                    && self.deficit(i) * Decimal::TWO > price
                    && self.class_deficit(i) * Decimal::TWO > price
                    && self.limit_room(i).is_none_or(|room| price <= room)
            })
            .max_by(|a, b| self.deficit(a.0).cmp(&self.deficit(b.0)))
    }

    /// O ativo fracionável mais atrasado, com ao menos um centavo de folga na
    /// meta dele, na da classe e no limite.
    fn next_fractional(&self) -> Option<usize> {
        let holdings = self.holdings;
        (0..holdings.len())
            .filter(|&i| holdings[i].is_eligible() && holdings[i].unit != UnitKind::Whole)
            .filter(|&i| {
                self.deficit(i) >= CENT
                    && self.class_deficit(i) >= CENT
                    && self.limit_room(i).is_none_or(|room| room >= CENT)
            })
            .max_by(|&a, &b| self.deficit(a).cmp(&self.deficit(b)))
    }

    fn buy(&mut self, i: usize, quantity: Decimal, value: Decimal) {
        self.planned_qty[i] += quantity;
        self.planned_value[i] += value;
        self.class_planned[class_index(self.holdings[i].asset_class)] += value;
    }
}

/// Os itens do plano: cada ativo que recebe, com a fatia dele na própria
/// classe depois do aporte.
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
        .collect()
}

/// Como cada classe fica: a meta, o antes e o depois, e o status.
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

/// O status da classe no plano, de uma meta `target_bp` e do que ela recebeu.
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

/// Os ativos da classe que podem receber, na ordem da carteira.
fn eligible_members(holdings: &[Holding], class: AssetClass) -> Vec<usize> {
    (0..holdings.len())
        .filter(|&i| holdings[i].asset_class == class && holdings[i].is_eligible())
        .collect()
}

/// Divide `base` entre os `members` pela nota, sem a fatia de ninguém passar
/// do próprio teto (`cap`). Quem bate no teto fica nele, e o resto é
/// redividido pela nota entre os outros — o excedente vai para os outros
/// ativos da classe.
///
/// Sem teto (`cap` sempre `None`) é a conta de sempre, a [`slice`] de cada um.
fn capped_shares(
    base: Decimal,
    score_sum: Decimal,
    members: &[usize],
    holdings: &[Holding],
    cap: impl Fn(&Holding) -> Option<Decimal>,
) -> Vec<Decimal> {
    let scores: Vec<Decimal> = members
        .iter()
        .map(|&i| Decimal::from(holdings[i].score))
        .collect();
    let caps: Vec<Option<Decimal>> = members.iter().map(|&i| cap(&holdings[i])).collect();
    let mut fixed = vec![false; members.len()];
    let mut free_base = base;
    let mut free_sum = score_sum;
    // Cada volta fixa ao menos um membro, ou para. Quem é fixado leva menos do
    // que a divisão lhe daria, então a fatia dos outros só cresce e ninguém
    // fixado antes volta a caber.
    for _ in 0..=members.len() {
        if free_sum <= Decimal::ZERO {
            break;
        }
        let mut changed = false;
        for slot in 0..members.len() {
            if let (false, Some(cap)) = (fixed[slot], caps[slot]) {
                if slice(free_base, holdings[members[slot]].score, free_sum) > cap {
                    fixed[slot] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
        free_base = base;
        free_sum = score_sum;
        for slot in (0..members.len()).filter(|&s| fixed[s]) {
            free_base -= caps[slot].unwrap_or(Decimal::ZERO);
            free_sum -= scores[slot];
        }
    }
    (0..members.len())
        .map(|slot| match caps[slot] {
            Some(cap) if fixed[slot] => cap,
            _ if free_sum > Decimal::ZERO => {
                slice(free_base, holdings[members[slot]].score, free_sum)
            }
            _ => Decimal::ZERO,
        })
        .collect()
}

/// O aviso do limite: quem parou nele e o que ficou sem destino por causa
/// dele — o que ainda faltava nas classes que pararam no que cabia nos ativos
/// (as bloqueadas pelo limite já têm o próprio aviso).
fn limit_notice(
    bp: u32,
    classes: &[ClassState; 7],
    holdings: &[Holding],
    at_limit: &[usize],
    unallocated: Decimal,
) -> Option<Notice> {
    let short: Decimal = classes
        .iter()
        .filter(|c| c.eligible && c.capacity.is_some_and(|cap| cap < c.gap))
        .map(|c| (c.gap - c.allocation).max(Decimal::ZERO))
        .sum();
    let amount = round_cents(short.min(unallocated));
    if at_limit.is_empty() && amount < CENT {
        return None;
    }
    Some(Notice::ConcentrationLimit {
        max_percent: percent(Decimal::from(bp), Decimal::from(FULL_BP)),
        assets: at_limit
            .iter()
            .map(|&i| (holdings[i].asset_id, holdings[i].label.clone()))
            .collect(),
        amount,
    })
}

fn notices(targets: Targets, classes: &[ClassState; 7], holdings: &[Holding]) -> Vec<Notice> {
    let mut out = Vec::new();
    if targets.sum() == 0 {
        out.push(Notice::NoTargets);
    }
    for class in AssetClass::ALL {
        let state = classes[class_index(class)];
        if targets.get(class) > 0 && !state.eligible && state.gap >= CENT {
            let members: Vec<&Holding> =
                holdings.iter().filter(|h| h.asset_class == class).collect();
            let reason = if members.is_empty() {
                BlockReason::NoAssets
            } else if state.candidates {
                // Tem quem receberia, mas todos já estão no limite.
                BlockReason::ConcentrationLimit
            } else if members
                .iter()
                .any(|h| h.score > 0 && h.price().is_some() && h.ceiling_unavailable().is_some())
            {
                BlockReason::CeilingUnavailable
            } else if members.iter().any(|h| h.score > 0 && h.above_ceiling()) {
                BlockReason::AboveCeiling
            } else if members.iter().any(|h| h.score > 0) {
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
    for holding in holdings.iter().filter(|h| h.score > 0) {
        match (holding.ceiling, holding.price()) {
            (Ceiling::Price { source, value }, Some(price)) if price > value => {
                out.push(Notice::AssetAboveCeiling {
                    asset_id: holding.asset_id,
                    label: holding.label.clone(),
                    price,
                    max_price: value,
                    source,
                });
            }
            (Ceiling::Unavailable { method, reason }, _) => {
                out.push(Notice::AssetCeilingUnavailable {
                    asset_id: holding.asset_id,
                    label: holding.label.clone(),
                    method,
                    reason,
                });
            }
            _ => {}
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
/// numa classe com ao menos um ativo com fatia a soma dos ideais é 100. Sem
/// nenhum, todos ficam em 0. O valor sai sem arredondar; quem exibe arredonda.
///
/// É a composição-alvo pela nota, e o teto de preço não entra: ele é filtro
/// temporário de compra (o ativo sai deste aporte, não da carteira que a
/// pessoa quer ter). Quem está acima do teto mantém o ideal dele, e a Carteira
/// mostra o selo "acima do teto" para explicar por que não recebe agora.
pub fn ideal_class_percents(holdings: &[Holding]) -> Vec<Decimal> {
    let mut score_sums = [Decimal::ZERO; 7];
    for holding in holdings.iter().filter(|h| h.has_slice()) {
        score_sums[class_index(holding.asset_class)] += Decimal::from(holding.score);
    }
    holdings
        .iter()
        .map(|holding| {
            let score_sum = score_sums[class_index(holding.asset_class)];
            if holding.has_slice() && score_sum > Decimal::ZERO {
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
            ceiling: Ceiling::None,
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
        let plan = plan(dec!(6000), sample_targets(), &holdings, Limits::default());
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
        let plan = plan(dec!(200000), sample_targets(), &holdings, Limits::default());
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
        let plan = plan(dec!(1000), Targets::default(), &holdings, Limits::default());
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
        let plan = plan(dec!(150), targets, &holdings, Limits::default());
        assert_money_adds_up(&plan);
        assert_eq!(
            item(&plan, &holdings, "B11").map(|i| i.quantity),
            Some(dec!(1))
        );
        assert_eq!(plan.change, dec!(50));
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
        let plan = plan(dec!(1000), targets, &holdings, Limits::default());
        assert_money_adds_up(&plan);
        assert!(plan.items.is_empty(), "{:#?}", plan.items);
        assert_eq!(plan.change, dec!(1000));
        assert_eq!(class(&plan, BrReits).status, ClassStatus::AboveTarget);
        assert_eq!(class(&plan, BrStocks).status, ClassStatus::BelowTarget);
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
        let plan = plan(dec!(1000), targets, &holdings, Limits::default());
        assert_money_adds_up(&plan);
        let bond = item(&plan, &holdings, "IPCA").expect("o titulo recebe");
        // R$ 1.000 a R$ 4.248,81 o título: 0,23 título (0,2353… para baixo),
        // R$ 977,22. O que não coube em centésimo de título volta como troco.
        assert_eq!(bond.quantity, dec!(0.23));
        assert_eq!(bond.value, dec!(977.22));
        assert!(bond.divisible);
        assert_eq!(plan.change, dec!(22.78));
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
        let plan = plan(dec!(333.33), targets, &holdings, Limits::default());
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
        let plan = plan(dec!(-10), sample_targets(), &holdings, Limits::default());
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
        let plan = plan(dec!(1000), targets, &holdings, Limits::default());
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

        let blocked = plan(dec!(5000), targets, &concentrated, Limits::default());
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
        let plan = plan(dec!(5000), targets, &listed, Limits::default());
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
        let plan = plan(dec!(1000), targets, &holdings, Limits::default());
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

    /// O ideal é a composição-alvo pela nota; o teto é filtro de compra deste
    /// aporte. Acima do teto, o ativo não recebe, mas a fatia dele continua.
    #[test]
    fn the_ceiling_does_not_change_the_ideal_percent() {
        use AssetClass::*;
        let free = vec![
            holding(BrStocks, "CARA3", dec!(500), Some(dec!(32.50)), 8),
            holding(BrStocks, "BOA3", dec!(500), Some(dec!(20)), 4),
        ];
        let mut capped = free.clone();
        capped[0].ceiling = manual(dec!(30));
        assert!(capped[0].above_ceiling() && !capped[0].is_eligible());
        assert_eq!(ideal_class_percents(&capped), ideal_class_percents(&free));
        assert_eq!(ideal_class_percents(&capped)[0].round_dp(2), dec!(66.67));
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
        let plan = plan(dec!(300), targets, &holdings, Limits::default());
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

    fn manual(value: Decimal) -> Ceiling {
        Ceiling::Price {
            source: CeilingSource::Manual,
            value,
        }
    }

    fn with_ceiling(holding: Holding, max_price: Decimal) -> Holding {
        Holding {
            ceiling: manual(max_price),
            ..holding
        }
    }

    fn limit(percent: u32) -> Limits {
        Limits {
            max_asset_bp: Some(percent * 100),
        }
    }

    fn reason_of(plan: &Plan, class: AssetClass) -> Option<BlockReason> {
        plan.notices.iter().find_map(|n| match n {
            Notice::ClassBlocked {
                asset_class,
                reason,
                ..
            } if *asset_class == class => Some(*reason),
            _ => None,
        })
    }

    /// Carteira fictícia de R$ 10.000 do exemplo de `docs/alocacao.md`: metade
    /// em renda fixa (na meta), metade em ações abaixo da meta, e a nota 8 de
    /// AAAA3 puxando quase todo o aporte para ele.
    fn limit_example() -> (Vec<Holding>, Targets) {
        use AssetClass::*;
        let holdings = vec![
            holding(FixedIncome, "Tesouro", dec!(6000), Some(Decimal::ONE), 5),
            holding(BrStocks, "AAAA3", dec!(2000), Some(dec!(10)), 8),
            holding(BrStocks, "BBBB3", dec!(2000), Some(dec!(10)), 2),
        ];
        let mut targets = Targets::default();
        targets.set(BrStocks, 5000);
        targets.set(FixedIncome, 5000);
        (holdings, targets)
    }

    #[test]
    fn an_asset_above_its_ceiling_sits_out_and_says_why() {
        use AssetClass::*;
        let holdings = vec![
            with_ceiling(
                holding(BrStocks, "CARA3", dec!(500), Some(dec!(32.50)), 8),
                dec!(30),
            ),
            with_ceiling(
                holding(BrStocks, "BOA3", dec!(500), Some(dec!(20)), 4),
                dec!(25),
            ),
        ];
        let mut targets = Targets::default();
        targets.set(BrStocks, FULL_BP);
        let first = plan(dec!(1000), targets, &holdings, Limits::default());
        assert_money_adds_up(&first);
        assert!(item(&first, &holdings, "CARA3").is_none());
        // Fora da divisão, CARA3 ainda ocupa o espaço dele: BOA3 vai a
        // R$ 2.000 − R$ 500 = R$ 1.500, e recebe o aporte inteiro.
        assert_eq!(
            item(&first, &holdings, "BOA3").map(|i| i.quantity),
            Some(dec!(50))
        );
        assert!(first.notices.contains(&Notice::AssetAboveCeiling {
            asset_id: holdings[0].asset_id,
            label: "CARA3".into(),
            price: dec!(32.50),
            max_price: dec!(30),
            source: CeilingSource::Manual,
        }));
        // Preço igual ao teto ainda compra.
        let at = vec![with_ceiling(
            holding(BrStocks, "CARA3", dec!(500), Some(dec!(30)), 8),
            dec!(30),
        )];
        let plan = plan(dec!(300), targets, &at, Limits::default());
        assert_eq!(
            item(&plan, &at, "CARA3").map(|i| i.quantity),
            Some(dec!(10))
        );
        assert!(!plan
            .notices
            .iter()
            .any(|n| matches!(n, Notice::AssetAboveCeiling { .. })));
    }

    /// O teto pelo Graham de uma ação com prejuízo fica sem valor: a ação sai
    /// do aporte com o motivo, em vez de cair calada para "sem teto".
    #[test]
    fn a_method_ceiling_without_value_sits_out_with_the_method_and_the_reason() {
        use AssetClass::*;
        let unavailable = Ceiling::Unavailable {
            method: CeilingMethod::Graham,
            reason: ReferenceGap::Loss,
        };
        let holdings = vec![
            Holding {
                ceiling: unavailable,
                ..holding(BrStocks, "PREJ3", dec!(500), Some(dec!(10)), 8)
            },
            holding(BrStocks, "BOA3", dec!(500), Some(dec!(20)), 4),
        ];
        let mut targets = Targets::default();
        targets.set(BrStocks, FULL_BP);
        let first = plan(dec!(1000), targets, &holdings, Limits::default());
        assert_money_adds_up(&first);
        assert!(item(&first, &holdings, "PREJ3").is_none());
        // Fora da divisão, PREJ3 ocupa o espaço dele, como o ativo acima do teto.
        assert_eq!(
            item(&first, &holdings, "BOA3").map(|i| i.quantity),
            Some(dec!(50))
        );
        assert!(first.notices.contains(&Notice::AssetCeilingUnavailable {
            asset_id: holdings[0].asset_id,
            label: "PREJ3".into(),
            method: CeilingMethod::Graham,
            reason: ReferenceGap::Loss,
        }));
        // O ideal não muda: o teto é filtro deste aporte, não da carteira.
        assert_eq!(
            ideal_class_percents(&holdings),
            ideal_class_percents(&[
                holding(BrStocks, "PREJ3", dec!(500), Some(dec!(10)), 8),
                holdings[1].clone()
            ])
        );

        // Sozinha na classe, a classe fica bloqueada pelo teto sem valor.
        let alone = vec![
            holdings[0].clone(),
            holding(FixedIncome, "Tesouro", dec!(500), Some(Decimal::ONE), 5),
        ];
        let mut half = Targets::default();
        half.set(BrStocks, 5000);
        half.set(FixedIncome, 5000);
        let blocked = plan(dec!(1000), half, &alone, Limits::default());
        assert_eq!(
            reason_of(&blocked, BrStocks),
            Some(BlockReason::CeilingUnavailable)
        );
        assert_eq!(class(&blocked, BrStocks).status, ClassStatus::Blocked);
        assert_eq!(
            BlockReason::CeilingUnavailable.as_str(),
            "ceiling_unavailable"
        );
    }

    #[test]
    fn a_method_ceiling_with_value_works_like_a_manual_one() {
        use AssetClass::*;
        let by_method = |value| Ceiling::Price {
            source: CeilingSource::Method(CeilingMethod::Bazin),
            value,
        };
        let holdings = vec![Holding {
            ceiling: by_method(dec!(30)),
            ..holding(BrStocks, "CARA3", dec!(500), Some(dec!(32.50)), 8)
        }];
        let mut targets = Targets::default();
        targets.set(BrStocks, FULL_BP);
        let plan = plan(dec!(1000), targets, &holdings, Limits::default());
        assert!(plan.items.is_empty());
        assert!(plan.notices.contains(&Notice::AssetAboveCeiling {
            asset_id: holdings[0].asset_id,
            label: "CARA3".into(),
            price: dec!(32.50),
            max_price: dec!(30),
            source: CeilingSource::Method(CeilingMethod::Bazin),
        }));
    }

    #[test]
    fn a_class_with_every_asset_above_the_ceiling_is_blocked_and_the_money_goes_elsewhere() {
        use AssetClass::*;
        let holdings = vec![
            with_ceiling(
                holding(BrReits, "CARO11", dec!(100), Some(dec!(120)), 6),
                dec!(100),
            ),
            holding(BrReits, "SEMNOTA11", dec!(100), Some(dec!(90)), 0),
            holding(FixedIncome, "Tesouro", dec!(800), Some(Decimal::ONE), 5),
        ];
        let mut targets = Targets::default();
        targets.set(BrReits, 5000);
        targets.set(FixedIncome, 5000);
        let plan = plan(dec!(1000), targets, &holdings, Limits::default());
        assert_money_adds_up(&plan);
        assert_eq!(reason_of(&plan, BrReits), Some(BlockReason::AboveCeiling));
        assert_eq!(class(&plan, BrReits).status, ClassStatus::Blocked);
        // A renda fixa vai até a meta dela (R$ 1.000); o resto não tem destino.
        assert_eq!(class(&plan, FixedIncome).allocated, dec!(200));
        assert_eq!(plan.unallocated, dec!(800));
        assert_eq!(BlockReason::AboveCeiling.as_str(), "above_ceiling");
    }

    #[test]
    fn the_limit_sends_the_excess_to_the_other_assets_of_the_class() {
        let (holdings, targets) = limit_example();
        let units = |plan: &Plan, label: &str| item(plan, &holdings, label).map(|i| i.quantity);

        // Sem limite: AAAA3 vai a R$ 4.000, um terço da carteira.
        let free = plan(dec!(2000), targets, &holdings, Limits::default());
        assert_eq!(units(&free, "AAAA3"), Some(dec!(200)));
        assert_eq!(units(&free, "BBBB3"), None);

        // Com 30% (R$ 3.600): AAAA3 para no limite e o resto da classe vai
        // para BBBB3, que chega a R$ 2.400.
        let capped = plan(dec!(2000), targets, &holdings, limit(30));
        assert_money_adds_up(&capped);
        assert_eq!(units(&capped, "AAAA3"), Some(dec!(160)));
        assert_eq!(units(&capped, "BBBB3"), Some(dec!(40)));
        assert_eq!(capped.unallocated, Decimal::ZERO);
        assert!(capped.notices.contains(&Notice::ConcentrationLimit {
            max_percent: dec!(30),
            assets: vec![(holdings[1].asset_id, "AAAA3".into())],
            amount: Decimal::ZERO,
        }));
    }

    #[test]
    fn what_fits_nowhere_under_the_limit_is_left_over_with_the_reason() {
        let (holdings, targets) = limit_example();
        // Com 20% (R$ 2.400 de R$ 12.000), cada ação só cabe mais R$ 400; a
        // renda fixa já está na meta. R$ 1.200 ficam sem destino.
        let plan = plan(dec!(2000), targets, &holdings, limit(20));
        assert_money_adds_up(&plan);
        assert_eq!(class(&plan, AssetClass::BrStocks).allocated, dec!(800));
        assert_eq!(plan.unallocated, dec!(1200));
        let Some(Notice::ConcentrationLimit { amount, assets, .. }) = plan
            .notices
            .iter()
            .find(|n| matches!(n, Notice::ConcentrationLimit { .. }))
        else {
            unreachable!("sem o aviso do limite: {:#?}", plan.notices)
        };
        assert_eq!(*amount, dec!(1200));
        assert_eq!(assets.len(), 2);
    }

    #[test]
    fn what_a_class_cannot_take_under_the_limit_goes_to_another_class_below_target() {
        use AssetClass::*;
        let (mut holdings, _) = limit_example();
        holdings.push(holding(Crypto, "BTC", Decimal::ZERO, Some(dec!(300000)), 7));
        let mut targets = Targets::default();
        targets.set(BrStocks, 5000);
        targets.set(FixedIncome, 4000);
        targets.set(Crypto, 1000);
        // N = 12.000: ações faltam 2.000 mas só cabem 800 sob 20%; cripto
        // falta 1.200. Proporcional, cripto levaria 750; com o que as ações
        // não comportam, cripto vai até a meta dela.
        let plan = plan(dec!(2000), targets, &holdings, limit(20));
        assert_money_adds_up(&plan);
        assert_eq!(class(&plan, BrStocks).allocated, dec!(800));
        assert!(class(&plan, Crypto).allocated > dec!(1199));
        assert!(plan.unallocated < dec!(1));
    }

    #[test]
    fn a_class_whose_assets_are_all_at_the_limit_is_blocked() {
        use AssetClass::*;
        let holdings = vec![
            holding(BrStocks, "GRANDE3", dec!(5000), Some(dec!(10)), 8),
            holding(FixedIncome, "Tesouro", dec!(5000), Some(Decimal::ONE), 5),
        ];
        let mut targets = Targets::default();
        targets.set(BrStocks, 8000);
        targets.set(FixedIncome, 2000);
        let plan = plan(dec!(1000), targets, &holdings, limit(30));
        assert_money_adds_up(&plan);
        assert_eq!(
            reason_of(&plan, BrStocks),
            Some(BlockReason::ConcentrationLimit)
        );
        assert_eq!(class(&plan, BrStocks).status, ClassStatus::Blocked);
        assert!(plan.items.is_empty());
        assert_eq!(
            BlockReason::ConcentrationLimit.as_str(),
            "concentration_limit"
        );
    }

    /// Teto e limite valem para título do Tesouro também, e o centésimo de
    /// título arredonda para baixo sob o limite.
    #[test]
    fn ceiling_and_limit_apply_to_treasury_bonds_in_hundredths() {
        use AssetClass::*;
        let bond = as_treasury(holding(
            FixedIncome,
            "IPCA",
            Decimal::ZERO,
            Some(dec!(4248.81)),
            5,
        ));
        let holdings = vec![
            bond.clone(),
            holding(BrStocks, "ITSA4", dec!(1000), Some(dec!(10)), 5),
        ];
        let mut targets = Targets::default();
        targets.set(FixedIncome, 5000);
        targets.set(BrStocks, 5000);
        // N = 2.000; 40% = R$ 800. A renda fixa falta R$ 1.000 e só cabem 800.
        let capped = plan(dec!(1000), targets, &holdings, limit(40));
        assert_money_adds_up(&capped);
        let bought = item(&capped, &holdings, "IPCA").expect("o titulo recebe");
        // R$ 800 a R$ 4.248,81: 0,18 título (0,1882… para baixo), R$ 764,78.
        assert_eq!(bought.quantity, dec!(0.18));
        assert_eq!(bought.value, dec!(764.78));
        assert_eq!(capped.unallocated, dec!(200));

        let above = vec![with_ceiling(bond, dec!(4000)), holdings[1].clone()];
        let blocked = plan(dec!(1000), targets, &above, Limits::default());
        assert!(blocked.items.is_empty());
        assert_eq!(
            reason_of(&blocked, FixedIncome),
            Some(BlockReason::AboveCeiling)
        );
    }

    #[test]
    fn the_change_round_never_crosses_the_limit() {
        use AssetClass::*;
        // N = 1.000; 50% = R$ 500 por ativo. Sem limite, a segunda cota de
        // UNICA3 (R$ 600, meia cota acima da meta) sairia do troco; com o
        // limite, não: ali não há folga de meia unidade.
        let holdings = vec![
            holding(BrStocks, "UNICA3", Decimal::ZERO, Some(dec!(300)), 8),
            holding(BrReits, "CARO11", Decimal::ZERO, Some(dec!(900)), 5),
        ];
        let mut targets = Targets::default();
        targets.set(BrStocks, 5000);
        targets.set(BrReits, 5000);
        let units = |plan: &Plan| item(plan, &holdings, "UNICA3").map(|i| i.quantity);
        let free = plan(dec!(1000), targets, &holdings, Limits::default());
        assert_eq!(units(&free), Some(dec!(2)));
        let capped = plan(dec!(1000), targets, &holdings, limit(50));
        assert_money_adds_up(&capped);
        assert_eq!(units(&capped), Some(dec!(1)));
        assert_eq!(capped.change, dec!(700));
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

        /// Teto de preço em metade dos ativos com cotação, ora abaixo, ora
        /// acima do preço (de 50% a 150% dele), às vezes pelo método e às
        /// vezes com o método sem valor.
        fn arb_holding_with_ceiling() -> impl Strategy<Value = Holding> {
            (
                arb_holding(),
                prop::option::weighted(0.5, 50u32..150),
                prop::bool::weighted(0.3),
                prop::bool::weighted(0.15),
            )
                .prop_map(|(h, factor, by_method, unavailable)| {
                    let ceiling = match (h.unit, h.price(), factor) {
                        (UnitKind::Value, _, _) | (_, None, _) | (_, _, None) => Ceiling::None,
                        _ if unavailable => Ceiling::Unavailable {
                            method: CeilingMethod::Graham,
                            reason: ReferenceGap::Loss,
                        },
                        (_, Some(price), Some(f)) => Ceiling::Price {
                            source: if by_method {
                                CeilingSource::Method(CeilingMethod::Combined)
                            } else {
                                CeilingSource::Manual
                            },
                            value: (price * Decimal::from(f) / HUNDRED).round_dp(2).max(CENT),
                        },
                    };
                    Holding { ceiling, ..h }
                })
        }

        /// Limite de 1% a 100%, às vezes desligado.
        fn arb_limits() -> impl Strategy<Value = Limits> {
            prop::option::weighted(0.85, 100u32..=FULL_BP)
                .prop_map(|max_asset_bp| Limits { max_asset_bp })
        }

        fn assert_common_invariants(
            plan: &Plan,
            targets: Targets,
            holdings: &[Holding],
            amount: Decimal,
        ) -> Result<(), TestCaseError> {
            prop_assert_eq!(plan.amount, plan.invested + plan.change + plan.unallocated);
            prop_assert!(plan.change >= Decimal::ZERO);
            prop_assert!(plan.unallocated >= Decimal::ZERO);
            for item in &plan.items {
                let h = holdings
                    .iter()
                    .find(|h| h.asset_id == item.asset_id)
                    .expect("item de um ativo");
                prop_assert!(h.score > 0, "nota zero recebeu");
                prop_assert!(item.value > Decimal::ZERO);
                // Com teto, nenhum ativo acima do teto recebe.
                prop_assert!(
                    h.max_price().is_none_or(|max| item.unit_price <= max),
                    "{} acima do teto recebeu: {} > {:?}",
                    h.label,
                    item.unit_price,
                    h.max_price()
                );
                // Teto pelo método sem valor: o ativo nunca recebe.
                prop_assert!(
                    h.ceiling_unavailable().is_none(),
                    "{} com teto sem valor recebeu",
                    h.label
                );
            }
            assert_classes_never_pass_their_target(plan, targets, holdings, amount)
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
                    if members.iter().any(|&i| holdings[i].has_slice()) {
                        prop_assert!((sum - HUNDRED).abs() < dec!(0.000001), "soma {sum}");
                    } else {
                        prop_assert_eq!(sum, Decimal::ZERO);
                    }
                    for &i in &members {
                        prop_assert!(ideal[i] >= Decimal::ZERO && ideal[i] <= HUNDRED);
                        if !holdings[i].has_slice() {
                            prop_assert_eq!(ideal[i], Decimal::ZERO);
                        }
                    }
                }
            }

            #[test]
            fn money_always_adds_up_and_nobody_overshoots(
                holdings in prop::collection::vec(arb_holding_with_ceiling(), 0..12),
                targets in arb_targets(),
                amount_cents in 0u32..10_000_000,
            ) {
                let amount = Decimal::new(i64::from(amount_cents), 2);
                let plan = plan(amount, targets, &holdings, Limits::default());
                assert_common_invariants(&plan, targets, &holdings, amount)?;
                for item in &plan.items {
                    let h = holdings.iter().find(|h| h.asset_id == item.asset_id).expect("item de um ativo");
                    // Nenhum ativo passa da própria meta por mais de meia unidade.
                    // Quem está acima do teto ocupa espaço na classe, como quem
                    // não tem nota ou preço.
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
            }

            /// O limite é teto duro: a conta arredonda sempre para baixo (cota
            /// inteira, 8 casas, centavo) e o troco só compra a unidade que
            /// cabe inteira sob ele. A única folga é o centavo do valor
            /// arredondado do item. Quem já estava acima do limite antes do
            /// aporte não recebe (o motor não vende).
            #[test]
            fn with_a_limit_no_asset_ends_above_it(
                holdings in prop::collection::vec(arb_holding_with_ceiling(), 0..12),
                targets in arb_targets(),
                limits in arb_limits(),
                amount_cents in 0u32..10_000_000,
            ) {
                let amount = Decimal::new(i64::from(amount_cents), 2);
                let plan = plan(amount, targets, &holdings, limits);
                assert_common_invariants(&plan, targets, &holdings, amount)?;
                let Some(bp) = limits.max_asset_bp else { return Ok(()) };
                let n: Decimal = holdings.iter().map(|h| h.value).sum::<Decimal>() + amount;
                let cap = n * Decimal::from(bp) / Decimal::from(FULL_BP);
                for item in &plan.items {
                    let h = holdings.iter().find(|h| h.asset_id == item.asset_id).expect("item de um ativo");
                    prop_assert!(
                        h.value + item.value <= cap + CENT,
                        "{} passou do limite: {} > {}", h.label, h.value + item.value, cap
                    );
                }
            }

            /// Sem teto e sem limite, o motor dá o mesmo plano, centavo por
            /// centavo e aviso por aviso, que o motor de antes das duas regras.
            #[test]
            fn without_ceiling_or_limit_nothing_changes(
                holdings in prop::collection::vec(arb_holding(), 0..12),
                targets in arb_targets(),
                amount_cents in 0u32..10_000_000,
            ) {
                let amount = Decimal::new(i64::from(amount_cents), 2);
                prop_assert_eq!(
                    plan(amount, targets, &holdings, Limits::default()),
                    legacy::plan(amount, targets, &holdings)
                );
            }

            /// Limite de 100% não corta ninguém: nenhum ativo passa da carteira
            /// inteira.
            #[test]
            fn a_limit_of_one_hundred_percent_changes_nothing(
                holdings in prop::collection::vec(arb_holding_with_ceiling(), 0..12),
                targets in arb_targets(),
                amount_cents in 0u32..10_000_000,
            ) {
                let amount = Decimal::new(i64::from(amount_cents), 2);
                let full = Limits { max_asset_bp: Some(FULL_BP) };
                let mut limited = plan(amount, targets, &holdings, full);
                limited.notices.retain(|n| !matches!(n, Notice::ConcentrationLimit { amount, .. } if amount.is_zero()));
                prop_assert_eq!(limited, plan(amount, targets, &holdings, Limits::default()));
            }
        }
    }
}
