//! Provento fora do padrão: o valor por cota declarado numa data com contra o
//! que o ativo costuma pagar. Regra pura; o porquê de cada escolha está em
//! `docs/agenda.md` ("Provento fora do padrão").
//!
//! - **O pagamento.** O que a empresa declarou numa data com, por cota **de
//!   hoje** (a base da estimativa da Renda, `income::estimate::shares_after`),
//!   bruto, com as parcelas somadas: separado **por tipo** (dividendo, JCP,
//!   rendimento de FII) e no total.
//! - **Corte ou aumento de um pagamento regular** ([`Basis::Similar`]): o
//!   valor de um tipo contra a média dos pagamentos **parecidos** do mesmo tipo
//!   (de metade ao dobro) nos 12 meses antes da data ex, com pelo menos
//!   [`MIN_RECENT`] parecidos. Parecido porque muita empresa paga em duas
//!   cadências (um banco com JCP mensal de R$ 0,02 e complementos de R$ 0,35).
//! - **Corte do pagamento regular sem parecido** ([`Basis::Regular`]): o valor
//!   abaixo de todos os do tipo nos 12 meses só é corte se **ocupa o lugar** do
//!   pagamento regular: o tipo tem cadência (pelo menos [`MIN_RECENT`]
//!   pagamentos, intervalo típico de até [`MAX_CADENCE_DAYS`] dias) e o
//!   pagamento anterior ficou a pelo menos [`SLOT_SHARE`] desse intervalo. O FII
//!   que paga 0,30 no lugar dos 0,80 do mês é corte; o resíduo que cai três meses
//!   depois do dividendo anual é um pagamento avulso: nada.
//! - **Aumento extraordinário** ([`Basis::Largest`]): o total da data com passa
//!   de `limiar ×` o **maior** pagamento dos 12 meses (com pelo menos
//!   [`MIN_RECENT`] pagamentos neles). Pagamento sem padrão nunca vira corte.
//! - **Os 5 anos confirmam.** Com 2 anos ou mais de histórico (a B3 e, antes
//!   dela, o Yahoo, que não separa o tipo), o total da data com tem de dar o
//!   mesmo veredito contra os pagamentos de até 5 anos, com pelo menos
//!   [`MIN_LONG`] na referência.
//! - **O limiar.** FII: 1,25 (+25% ou −20%), porque distribui quase todo o
//!   resultado e o rendimento mês a mês varia pouco. Ação e o resto: 1,5 (+50%
//!   ou −33%), o mesmo "acima do habitual" da Renda.

use rust_decimal::Decimal;
use time::{Date, Duration};

use crate::domain::asset_class::AssetClass;
use crate::domain::income::estimate::{
    UnitPayment, ABOVE_USUAL_RATIO, MIN_USUAL_YEARS, USUAL_YEARS,
};
use crate::domain::income::{IncomeKind, TRAILING_DAYS};

/// O limiar do FII: 1,25 × o habitual para cima, ÷ 1,25 (−20%) para baixo.
pub const FUND_RATIO: Decimal = Decimal::from_parts(125, 0, 0, false, 2);

/// O limiar de ação e do resto: o mesmo 1,5 do "acima do habitual" da Renda.
pub const STOCK_RATIO: Decimal = ABOVE_USUAL_RATIO;

/// Parecido é de `1 ÷ SIMILAR` a `SIMILAR ×` o valor.
pub const SIMILAR: Decimal = Decimal::TWO;

/// O limiar da classe.
pub fn ratio_for(class: AssetClass) -> Decimal {
    match class {
        AssetClass::BrReits | AssetClass::UsReits => FUND_RATIO,
        _ => STOCK_RATIO,
    }
}

/// O que uma cota (de hoje) recebeu numa data ex, somadas as linhas dela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Payment {
    pub ex_date: Date,
    /// Bruto, por cota de hoje, em reais.
    pub per_unit: Decimal,
}

/// Junta os pagamentos da mesma data ex (o JCP e o dividendo de uma data com
/// são um pagamento só) e ordena pela data. Valor zero ou negativo fica fora.
pub fn by_ex_date(payments: &[UnitPayment]) -> Vec<Payment> {
    let mut list: Vec<Payment> = Vec::new();
    let mut sorted: Vec<&UnitPayment> = payments.iter().collect();
    sorted.sort_by_key(|p| p.ex_date);
    for p in sorted {
        match list.last_mut() {
            Some(last) if last.ex_date == p.ex_date => last.per_unit += p.gross,
            _ => list.push(Payment {
                ex_date: p.ex_date,
                per_unit: p.gross,
            }),
        }
    }
    list.retain(|p| p.per_unit > Decimal::ZERO);
    list
}

/// O histórico de um ativo da B3: a B3 desde o primeiro anúncio que ela tem e,
/// antes disso, o Yahoo (a B3 cobre uns 12 a 18 meses). As datas ex batem nas
/// duas fontes (`docs/proventos-automaticos.md`, "A janela").
pub fn merged(b3: &[Payment], yahoo: &[Payment]) -> Vec<Payment> {
    let Some(start) = b3.iter().map(|p| p.ex_date).min() else {
        return yahoo.to_vec();
    };
    let mut list: Vec<Payment> = yahoo
        .iter()
        .filter(|p| p.ex_date < start)
        .copied()
        .chain(b3.iter().copied())
        .collect();
    list.sort_by_key(|p| p.ex_date);
    list
}

/// Quantos pagamentos a referência dos 12 meses precisa (parecidos, ou do tipo
/// para o corte do pagamento regular, ou no total para o aumento extraordinário).
/// Um só não é padrão: o pagador anual tem um por ano e não é avaliado.
pub const MIN_RECENT: usize = 2;

/// Quantos pagamentos a referência de até 5 anos precisa.
pub const MIN_LONG: usize = 3;

/// O intervalo típico mais longo que ainda é cadência (semestral, com folga).
pub const MAX_CADENCE_DAYS: i64 = 200;

/// O pagamento ocupa o lugar do regular quando o anterior ficou a pelo menos
/// esta fração do intervalo típico (60%: o mensal que atrasa uma semana conta).
pub const SLOT_SHARE: Decimal = Decimal::from_parts(6, 0, 0, false, 1);

/// Os tipos que formam padrão: dividendo, JCP e rendimento de FII. Amortização
/// devolve capital, e o "outro" da B3 numa ação é a correção do valor entre a
/// data com e o pagamento (centavos de centavo): ficam fora da comparação por
/// tipo e entram só no total.
pub fn judged(kind: IncomeKind) -> bool {
    matches!(
        kind,
        IncomeKind::Dividend | IncomeKind::Jcp | IncomeKind::ReitIncome
    )
}

/// Contra o que o valor foi comparado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// A média dos pagamentos parecidos do mesmo tipo.
    Similar,
    /// A mediana do tipo: o pagamento ocupa o lugar do regular e ficou abaixo
    /// de todos.
    Regular,
    /// O maior pagamento do período (aumento extraordinário).
    Largest,
}

/// A referência de um período.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reference {
    /// O habitual por cota: a média dos parecidos, a mediana do tipo ou o maior.
    pub usual: Decimal,
    /// `valor ÷ habitual − 1`: `0,25` é 25% acima.
    pub change: Decimal,
    /// Quantos pagamentos formaram a referência.
    pub payments: usize,
}

fn reference(value: Decimal, usual: Decimal, payments: usize) -> Option<Reference> {
    let change = (value.checked_div(usual)? - Decimal::ONE).round_dp(4);
    Some(Reference {
        usual: usual.round_dp(8),
        change,
        payments,
    })
}

/// Para cima ou para baixo do habitual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Increase,
    Cut,
}

/// O sentido, pelo habitual sem arredondar (o arredondado é o que a tela mostra).
fn direction(value: Decimal, usual: Decimal, ratio: Decimal) -> Option<Direction> {
    if value >= usual * ratio {
        Some(Direction::Increase)
    } else if value * ratio <= usual {
        Some(Direction::Cut)
    } else {
        None
    }
}

/// O que uma data com declarou, por cota de hoje: por tipo e no total (as
/// parcelas somadas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub ex_date: Date,
    pub by_kind: Vec<(IncomeKind, Decimal)>,
    pub total: Decimal,
}

/// O histórico de um ativo: por tipo (só a B3 diz o tipo) e no total (a B3 e,
/// antes dela, o Yahoo, [`merged`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    pub by_kind: Vec<(IncomeKind, Vec<Payment>)>,
    pub totals: Vec<Payment>,
}

impl History {
    fn of(&self, kind: IncomeKind) -> &[Payment] {
        self.by_kind
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(&[][..], |(_, list)| list.as_slice())
    }
}

/// Um provento fora do padrão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unusual {
    pub direction: Direction,
    /// O valor comparado, por cota de hoje: o do tipo, ou o total no aumento
    /// extraordinário.
    pub per_unit: Decimal,
    /// O tipo comparado; `None` no aumento extraordinário (o total).
    pub kind: Option<IncomeKind>,
    pub basis: Basis,
    /// Os 12 meses antes da data ex.
    pub recent: Reference,
    /// Até 5 anos antes da data ex, no total; `None` com menos de 2 anos.
    pub long: Option<Reference>,
    /// Quantos anos inteiros o período longo cobre (2 a 5), ou 0 sem ele.
    pub years: u32,
}

/// Os pagamentos com data ex em `[ex_date − days, ex_date)`, em ordem.
fn window(history: &[Payment], ex_date: Date, days: i64) -> Vec<Payment> {
    let start = ex_date.saturating_sub(Duration::days(days));
    let mut list: Vec<Payment> = history
        .iter()
        .filter(|p| p.ex_date >= start && p.ex_date < ex_date)
        .copied()
        .collect();
    list.sort_by_key(|p| p.ex_date);
    list
}

fn median(values: &mut [Decimal]) -> Option<Decimal> {
    values.sort();
    let n = values.len();
    if n == 0 {
        return None;
    }
    if n % 2 == 1 {
        return values.get(n / 2).copied();
    }
    Some((*values.get(n / 2 - 1)? + *values.get(n / 2)?) / Decimal::TWO)
}

/// Se o pagamento com data ex `ex_date` ocupa o lugar do regular: a lista tem
/// cadência (intervalo típico de até [`MAX_CADENCE_DAYS`]) e o anterior ficou
/// a pelo menos [`SLOT_SHARE`] dela.
fn regular_slot(window: &[Payment], ex_date: Date) -> bool {
    let mut gaps: Vec<Decimal> = window
        .windows(2)
        .filter_map(|pair| match pair {
            [a, b] => Some(Decimal::from((b.ex_date - a.ex_date).whole_days())),
            _ => None,
        })
        .collect();
    let (Some(typical), Some(last)) = (median(&mut gaps), window.last()) else {
        return false;
    };
    let since = Decimal::from((ex_date - last.ex_date).whole_days());
    typical > Decimal::ZERO
        && typical <= Decimal::from(MAX_CADENCE_DAYS)
        && since >= typical * SLOT_SHARE
}

/// O veredito de um valor contra uma janela: pelos parecidos (com pelo menos
/// `min`), ou, abaixo de todos e no lugar do regular, pela mediana (só corte).
fn judge(
    value: Decimal,
    window: &[Payment],
    ex_date: Date,
    ratio: Decimal,
    min: usize,
) -> Option<(Direction, Reference, Basis)> {
    if value <= Decimal::ZERO {
        return None;
    }
    let low = value / SIMILAR;
    let high = value * SIMILAR;
    let similar: Vec<Decimal> = window
        .iter()
        .map(|p| p.per_unit)
        .filter(|v| *v >= low && *v <= high)
        .collect();
    if similar.len() >= min {
        let usual = similar.iter().sum::<Decimal>() / Decimal::from(similar.len());
        let found = direction(value, usual, ratio)?;
        return Some((
            found,
            reference(value, usual, similar.len())?,
            Basis::Similar,
        ));
    }
    let below_all = window.iter().all(|p| value < p.per_unit);
    if window.len() >= min && below_all && regular_slot(window, ex_date) {
        let mut values: Vec<Decimal> = window.iter().map(|p| p.per_unit).collect();
        let usual = median(&mut values)?;
        if direction(value, usual, ratio) == Some(Direction::Cut) {
            return Some((
                Direction::Cut,
                reference(value, usual, window.len())?,
                Basis::Regular,
            ));
        }
    }
    None
}

/// O aumento extraordinário: acima de `ratio ×` o maior da janela (com pelo
/// menos `min` pagamentos nela).
fn largest(value: Decimal, window: &[Payment], ratio: Decimal, min: usize) -> Option<Reference> {
    if window.len() < min {
        return None;
    }
    let top = window.iter().map(|p| p.per_unit).max()?;
    if top > Decimal::ZERO && value >= top * ratio {
        reference(value, top, window.len())
    } else {
        None
    }
}

/// Quantos anos inteiros o histórico total cobre antes da data ex (até 5).
fn years_before(history: &History, ex_date: Date) -> u32 {
    history
        .totals
        .iter()
        .map(|p| p.ex_date)
        .min()
        .map_or(0, |first| {
            let span = (ex_date - first).whole_days() / TRAILING_DAYS;
            u32::try_from(span).unwrap_or(0).min(USUAL_YEARS)
        })
}

/// Se a data com `declaration` está fora do padrão do `history`, com o limiar
/// `ratio` ([`ratio_for`]).
pub fn unusual(declaration: &Declaration, history: &History, ratio: Decimal) -> Option<Unusual> {
    let ex_date = declaration.ex_date;
    let years = years_before(history, ex_date);
    let long_window = (years >= MIN_USUAL_YEARS)
        .then(|| window(&history.totals, ex_date, TRAILING_DAYS * i64::from(years)));

    for (kind, value) in declaration
        .by_kind
        .iter()
        .filter(|(kind, value)| judged(*kind) && *value > Decimal::ZERO)
    {
        let recent = window(history.of(*kind), ex_date, TRAILING_DAYS);
        let Some((found, recent_ref, basis)) = judge(*value, &recent, ex_date, ratio, MIN_RECENT)
        else {
            continue;
        };
        let long = match &long_window {
            None => None,
            Some(list) => match judge(declaration.total, list, ex_date, ratio, MIN_LONG) {
                Some((same, long_ref, _)) if same == found => Some(long_ref),
                _ => continue,
            },
        };
        return Some(Unusual {
            direction: found,
            per_unit: *value,
            kind: Some(*kind),
            basis,
            recent: recent_ref,
            long,
            years: if long.is_some() { years } else { 0 },
        });
    }

    let recent_totals = window(&history.totals, ex_date, TRAILING_DAYS);
    let recent = largest(declaration.total, &recent_totals, ratio, MIN_RECENT)?;
    let long = match &long_window {
        None => None,
        Some(list) => Some(largest(declaration.total, list, ratio, MIN_LONG)?),
    };
    Some(Unusual {
        direction: Direction::Increase,
        per_unit: declaration.total,
        kind: None,
        basis: Basis::Largest,
        recent,
        long,
        years: if long.is_some() { years } else { 0 },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    const EX: Date = date!(2026 - 12 - 01);

    /// Um pagamento a cada `step` dias, de `start` até antes de `end`.
    fn every(start: Date, end: Date, step: i64, value: Decimal) -> Vec<Payment> {
        let mut out = Vec::new();
        let mut day = start;
        while day < end {
            out.push(Payment {
                ex_date: day,
                per_unit: value,
            });
            day = day.saturating_add(Duration::days(step));
        }
        out
    }

    fn sorted(mut list: Vec<Payment>) -> Vec<Payment> {
        list.sort_by_key(|p| p.ex_date);
        list
    }

    /// Um FII que paga `value` todo mês há seis anos (a B3 com o tipo nos
    /// últimos 18 meses; o total desde o começo).
    fn monthly_fund(value: Decimal) -> History {
        let all = every(date!(2020 - 11 - 02), EX, 30, value);
        let b3: Vec<Payment> = all
            .iter()
            .filter(|p| p.ex_date >= date!(2025 - 06 - 01))
            .copied()
            .collect();
        History {
            by_kind: vec![(IncomeKind::ReitIncome, b3)],
            totals: all,
        }
    }

    fn one(kind: IncomeKind, value: Decimal) -> Declaration {
        Declaration {
            ex_date: EX,
            by_kind: vec![(kind, value)],
            total: value,
        }
    }

    #[test]
    fn a_monthly_fund_cut_or_raised_is_out_of_pattern() {
        let history = monthly_fund(dec!(0.80));
        let cut = unusual(
            &one(IncomeKind::ReitIncome, dec!(0.60)),
            &history,
            FUND_RATIO,
        )
        .unwrap();
        assert_eq!(cut.direction, Direction::Cut);
        assert_eq!(cut.basis, Basis::Similar);
        assert_eq!(cut.kind, Some(IncomeKind::ReitIncome));
        assert_eq!(cut.recent.usual, dec!(0.80));
        assert_eq!(cut.recent.change, dec!(-0.25));
        assert!(cut.recent.payments >= MIN_RECENT);
        assert_eq!(cut.years, 5);
        assert!(cut.long.unwrap().payments >= MIN_LONG);
        // −18,75% fica dentro do habitual; +30% é aumento.
        assert!(unusual(
            &one(IncomeKind::ReitIncome, dec!(0.65)),
            &history,
            FUND_RATIO
        )
        .is_none());
        let up = unusual(
            &one(IncomeKind::ReitIncome, dec!(1.04)),
            &history,
            FUND_RATIO,
        )
        .unwrap();
        assert_eq!(up.direction, Direction::Increase);
        // Na ação, o limiar é 1,5: −25% fica dentro.
        assert!(unusual(
            &one(IncomeKind::ReitIncome, dec!(0.60)),
            &history,
            STOCK_RATIO
        )
        .is_none());
    }

    #[test]
    fn a_deep_cut_of_the_regular_payment_has_no_similar_and_still_counts() {
        // 0,30 no lugar dos 0,80 do mês: nenhum parecido, mas é o pagamento do mês.
        let history = monthly_fund(dec!(0.80));
        let cut = unusual(
            &one(IncomeKind::ReitIncome, dec!(0.30)),
            &history,
            FUND_RATIO,
        )
        .unwrap();
        assert_eq!(cut.direction, Direction::Cut);
        assert_eq!(cut.basis, Basis::Regular);
        assert_eq!(cut.recent.usual, dec!(0.80));
        assert_eq!(cut.recent.change, dec!(-0.625));
    }

    #[test]
    fn a_small_payment_against_one_big_payment_is_not_a_cut() {
        // Pagador anual de 1,1166 (cinco anos) e, três meses depois do último, um
        // resíduo de 0,0046: não há padrão de 12 meses para comparar.
        let annual = every(
            date!(2021 - 09 - 01),
            date!(2026 - 09 - 02),
            365,
            dec!(1.1166),
        );
        let history = History {
            by_kind: vec![(
                IncomeKind::Dividend,
                annual
                    .iter()
                    .filter(|p| p.ex_date >= date!(2025 - 06 - 01))
                    .copied()
                    .collect(),
            )],
            totals: annual.clone(),
        };
        assert!(unusual(
            &one(IncomeKind::Dividend, dec!(0.0046)),
            &history,
            STOCK_RATIO
        )
        .is_none());
        // Semestral de 0,73 e 0,70 e um resíduo de 0,0449 vinte dias depois do
        // último: abaixo de todos, mas fora do lugar do pagamento regular.
        let semiannual = sorted(vec![
            Payment {
                ex_date: date!(2026 - 05 - 11),
                per_unit: dec!(0.73),
            },
            Payment {
                ex_date: date!(2026 - 11 - 11),
                per_unit: dec!(0.70),
            },
        ]);
        let history = History {
            by_kind: vec![(IncomeKind::Dividend, semiannual.clone())],
            totals: semiannual.clone(),
        };
        assert!(unusual(
            &one(IncomeKind::Dividend, dec!(0.0449)),
            &history,
            STOCK_RATIO
        )
        .is_none());
        // O mesmo valor como JCP: outro tipo, sem histórico de JCP, nada.
        assert!(unusual(&one(IncomeKind::Jcp, dec!(0.0449)), &history, STOCK_RATIO).is_none());
        // E nunca contra um pagamento só: o tipo com um pagamento não forma padrão.
        let lone = History {
            by_kind: vec![(IncomeKind::Dividend, semiannual[1..].to_vec())],
            totals: semiannual[1..].to_vec(),
        };
        assert!(unusual(&one(IncomeKind::Dividend, dec!(0.20)), &lone, STOCK_RATIO).is_none());
    }

    #[test]
    fn an_extraordinary_declaration_in_installments_is_an_increase() {
        // JCP trimestral de 0,15 e dividendo semestral de 0,20; a data com declara
        // 3 × 0,4128 = 1,2384 de dividendo, pago em três parcelas.
        let jcp = every(date!(2025 - 12 - 15), EX, 91, dec!(0.15));
        let dividend = every(date!(2026 - 02 - 20), EX, 182, dec!(0.20));
        let totals = sorted(jcp.iter().chain(dividend.iter()).copied().collect());
        let history = History {
            by_kind: vec![(IncomeKind::Jcp, jcp), (IncomeKind::Dividend, dividend)],
            totals,
        };
        let declaration = one(IncomeKind::Dividend, dec!(1.2384));
        let up = unusual(&declaration, &history, STOCK_RATIO).unwrap();
        assert_eq!(up.direction, Direction::Increase);
        assert_eq!(up.basis, Basis::Largest);
        assert_eq!(up.kind, None);
        assert_eq!(up.recent.usual, dec!(0.20));
        assert_eq!(up.per_unit, dec!(1.2384));
        // Um pouco acima do maior não é extraordinário.
        assert!(unusual(
            &one(IncomeKind::Dividend, dec!(0.29)),
            &history,
            STOCK_RATIO
        )
        .is_none());
    }

    #[test]
    fn a_bank_with_monthly_jcp_and_semiannual_complements_stays_quiet() {
        let mut jcp = every(date!(2025 - 12 - 01), EX, 30, dec!(0.018));
        jcp.push(Payment {
            ex_date: date!(2026 - 03 - 20),
            per_unit: dec!(0.35),
        });
        jcp.push(Payment {
            ex_date: date!(2026 - 06 - 20),
            per_unit: dec!(0.36),
        });
        let jcp = sorted(jcp);
        let history = History {
            by_kind: vec![(IncomeKind::Jcp, jcp.clone())],
            totals: jcp,
        };
        for value in [dec!(0.0182), dec!(0.355)] {
            assert!(unusual(&one(IncomeKind::Jcp, value), &history, STOCK_RATIO).is_none());
        }
        // O complemento de 0,12 fica entre os dois padrões: avulso, nada.
        assert!(unusual(&one(IncomeKind::Jcp, dec!(0.12)), &history, STOCK_RATIO).is_none());
        // O JCP mensal que dobra é aumento.
        let up = unusual(&one(IncomeKind::Jcp, dec!(0.036)), &history, STOCK_RATIO).unwrap();
        assert_eq!(up.direction, Direction::Increase);
        assert_eq!(up.basis, Basis::Similar);
    }

    #[test]
    fn the_five_years_must_agree() {
        // Cinco anos de 1,00 e um de 0,70: voltar a 1,00 não é aumento.
        let mut all = every(date!(2020 - 11 - 02), date!(2025 - 12 - 01), 30, dec!(1.00));
        all.extend(every(date!(2025 - 12 - 01), EX, 30, dec!(0.70)));
        let history = History {
            by_kind: vec![(
                IncomeKind::ReitIncome,
                all.iter()
                    .filter(|p| p.ex_date >= date!(2025 - 06 - 01))
                    .copied()
                    .collect(),
            )],
            totals: all,
        };
        assert!(unusual(
            &one(IncomeKind::ReitIncome, dec!(1.00)),
            &history,
            FUND_RATIO
        )
        .is_none());
        assert!(unusual(
            &one(IncomeKind::ReitIncome, dec!(0.70)),
            &history,
            FUND_RATIO
        )
        .is_none());
    }

    #[test]
    fn short_history_uses_the_twelve_months_alone() {
        let b3 = every(date!(2026 - 03 - 02), EX, 30, dec!(0.10));
        let history = History {
            by_kind: vec![(IncomeKind::ReitIncome, b3.clone())],
            totals: b3,
        };
        let found = unusual(
            &one(IncomeKind::ReitIncome, dec!(0.07)),
            &history,
            FUND_RATIO,
        )
        .unwrap();
        assert_eq!(found.direction, Direction::Cut);
        assert!(found.long.is_none());
        assert_eq!(found.years, 0);
        // Sem nada antes da data ex, ou valor zero: nada.
        let early = Declaration {
            ex_date: date!(2026 - 01 - 01),
            ..one(IncomeKind::ReitIncome, dec!(0.07))
        };
        assert!(unusual(&early, &history, FUND_RATIO).is_none());
        assert!(unusual(&one(IncomeKind::ReitIncome, dec!(0)), &history, FUND_RATIO).is_none());
        // Amortização e o "outro" não formam padrão.
        assert!(!judged(IncomeKind::Amortization));
        assert!(!judged(IncomeKind::Other));
    }

    #[test]
    fn the_same_date_is_one_payment_and_b3_wins_over_yahoo() {
        let rows = [
            UnitPayment {
                ex_date: date!(2026 - 09 - 02),
                pay_date: None,
                gross: dec!(0.10),
                net: dec!(0.0825),
            },
            UnitPayment {
                ex_date: date!(2026 - 09 - 02),
                pay_date: None,
                gross: dec!(0.03),
                net: dec!(0.03),
            },
            UnitPayment {
                ex_date: date!(2026 - 06 - 02),
                pay_date: None,
                gross: dec!(0.14),
                net: dec!(0.14),
            },
        ];
        let b3 = by_ex_date(&rows);
        assert_eq!(
            b3,
            vec![
                Payment {
                    ex_date: date!(2026 - 06 - 02),
                    per_unit: dec!(0.14)
                },
                Payment {
                    ex_date: date!(2026 - 09 - 02),
                    per_unit: dec!(0.13)
                },
            ]
        );
        let yahoo = vec![
            Payment {
                ex_date: date!(2025 - 12 - 02),
                per_unit: dec!(0.20),
            },
            Payment {
                ex_date: date!(2026 - 06 - 02),
                per_unit: dec!(9.99),
            },
        ];
        let all = merged(&b3, &yahoo);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].per_unit, dec!(0.20));
        assert_eq!(all[1].per_unit, dec!(0.14));
        assert_eq!(merged(&[], &yahoo), yahoo);
        assert_eq!(ratio_for(AssetClass::BrReits), FUND_RATIO);
        assert_eq!(ratio_for(AssetClass::BrStocks), STOCK_RATIO);
        let mut even = vec![dec!(4), dec!(1), dec!(3), dec!(2)];
        assert_eq!(median(&mut even), Some(dec!(2.5)));
        assert_eq!(median(&mut []), None);
    }

    fn history_strategy() -> impl Strategy<Value = Vec<Payment>> {
        proptest::collection::vec((0i64..2200, 1i64..100_000), 1..40).prop_map(|points| {
            sorted(
                points
                    .into_iter()
                    .map(|(back, cents)| Payment {
                        ex_date: EX.saturating_sub(Duration::days(back + 1)),
                        per_unit: Decimal::new(cents, 4),
                    })
                    .collect(),
            )
        })
    }

    fn as_history(list: &[Payment]) -> History {
        History {
            by_kind: vec![(IncomeKind::ReitIncome, list.to_vec())],
            totals: list.to_vec(),
        }
    }

    proptest! {
        /// Trocar a unidade (todos os valores × k) não muda o veredito.
        #[test]
        fn the_verdict_does_not_depend_on_the_unit(
            history in history_strategy(),
            cents in 1i64..100_000,
            k in 1i64..50,
        ) {
            let value = Decimal::new(cents, 4);
            let factor = Decimal::from(k);
            let scaled: Vec<Payment> = history
                .iter()
                .map(|p| Payment { ex_date: p.ex_date, per_unit: p.per_unit * factor })
                .collect();
            let a = unusual(&one(IncomeKind::ReitIncome, value), &as_history(&history), FUND_RATIO)
                .map(|u| (u.direction, u.basis));
            let b = unusual(&one(IncomeKind::ReitIncome, value * factor), &as_history(&scaled), FUND_RATIO)
                .map(|u| (u.direction, u.basis));
            prop_assert_eq!(a, b);
        }

        /// O sentido diz o sinal da mudança, que passa do limiar, e a referência
        /// nunca é um pagamento só.
        #[test]
        fn the_direction_matches_the_change(
            history in history_strategy(),
            cents in 1i64..100_000,
        ) {
            let value = Decimal::new(cents, 4);
            if let Some(found) = unusual(&one(IncomeKind::ReitIncome, value), &as_history(&history), STOCK_RATIO) {
                prop_assert!(found.recent.payments >= MIN_RECENT);
                if let Some(long) = found.long {
                    prop_assert!(long.payments >= MIN_LONG);
                }
                let slack = dec!(0.0001);
                for reference in std::iter::once(found.recent).chain(found.long) {
                    match found.direction {
                        Direction::Increase => {
                            prop_assert!(reference.change >= STOCK_RATIO - Decimal::ONE - slack);
                        }
                        Direction::Cut => {
                            let floor = Decimal::ONE / STOCK_RATIO - Decimal::ONE;
                            prop_assert!(reference.change <= floor + slack);
                        }
                    }
                }
            }
        }
    }
}
