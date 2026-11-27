//! Provento fora do padrão: o valor por cota declarado numa data com contra o
//! que o ativo costuma pagar. Regra pura; o porquê de cada escolha está em
//! `docs/agenda.md` ("Provento fora do padrão").
//!
//! - **O pagamento.** Tudo o que a empresa declarou com a mesma data com (JCP,
//!   dividendo, rendimento, cada parcela), por cota **de hoje** (a mesma base
//!   da estimativa da Renda, `income::estimate::shares_after`), bruto: o IR não
//!   muda o que é "fora do padrão".
//! - **O habitual.** A média dos pagamentos **parecidos** (de metade ao dobro
//!   do valor) nos 12 meses antes da data ex e nos 5 anos antes dela. Parecido
//!   porque muita empresa paga em duas cadências: um banco com JCP mensal e
//!   complementos semestrais tem pagamentos de R$ 0,02 e de R$ 0,35. A média
//!   de todos (R$ 0,065 num ano) faria todo JCP mensal parecer um corte de 72%
//!   (o padrão das páginas públicas da B3 em 27/11/2026). Sem nenhum pagamento parecido, o habitual é
//!   o pagamento mais próximo (na razão): o valor já está a mais do dobro dele.
//! - **O limiar.** Fora do padrão quando o valor passa de `ratio ×` o habitual
//!   (aumento) ou fica abaixo de `habitual ÷ ratio` (corte), **nos dois
//!   períodos, no mesmo sentido**. Com menos de 2 anos de histórico só os 12
//!   meses valem, como na média do habitual da Renda (`MIN_USUAL_YEARS`).
//!   FII: 1,25 (+25% ou −20%), porque distribui quase todo o resultado e o
//!   rendimento mês a mês varia pouco. Ação e o resto: 1,5 (+50% ou −33%), o
//!   mesmo "acima do habitual" da Renda, porque o provento por data com segue
//!   o lucro e o calendário da empresa.

use rust_decimal::Decimal;
use time::{Date, Duration};

use crate::domain::asset_class::AssetClass;
use crate::domain::income::estimate::{
    UnitPayment, ABOVE_USUAL_RATIO, MIN_USUAL_YEARS, USUAL_YEARS,
};
use crate::domain::income::TRAILING_DAYS;

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

/// O habitual de um período.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reference {
    /// A média dos pagamentos parecidos (ou o mais próximo), por cota.
    pub usual: Decimal,
    /// `valor ÷ habitual − 1`: `0,25` é 25% acima.
    pub change: Decimal,
    /// Quantos pagamentos entraram na média (1 quando foi o mais próximo).
    pub payments: usize,
}

/// A distância de `a` a `b` na razão (sempre ≥ 1).
fn distance(a: Decimal, b: Decimal) -> Option<Decimal> {
    let (high, low) = if a >= b { (a, b) } else { (b, a) };
    high.checked_div(low)
}

/// A média dos parecidos com `value` em `window` (ou o mais próximo), sem
/// arredondar, e quantos entraram.
fn usual_of(value: Decimal, window: &[Payment]) -> Option<(Decimal, usize)> {
    if value <= Decimal::ZERO || window.is_empty() {
        return None;
    }
    let low = value / SIMILAR;
    let high = value * SIMILAR;
    let similar: Vec<Decimal> = window
        .iter()
        .map(|p| p.per_unit)
        .filter(|v| *v >= low && *v <= high)
        .collect();
    if similar.is_empty() {
        let nearest = window
            .iter()
            .map(|p| p.per_unit)
            .filter_map(|v| distance(v, value).map(|d| (d, v)))
            .min_by(|a, b| a.0.cmp(&b.0))?
            .1;
        return Some((nearest, 1));
    }
    let count = Decimal::from(similar.len());
    Some((similar.iter().sum::<Decimal>() / count, similar.len()))
}

/// O habitual de `value` entre os pagamentos `window` (já recortados).
pub fn reference(value: Decimal, window: &[Payment]) -> Option<Reference> {
    let (usual, payments) = usual_of(value, window)?;
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

/// Um provento fora do padrão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unusual {
    pub direction: Direction,
    /// O valor por cota de hoje que a data com pagou.
    pub per_unit: Decimal,
    /// Os 12 meses antes da data ex.
    pub recent: Reference,
    /// Até 5 anos antes da data ex; `None` com menos de 2 anos de histórico.
    pub long: Option<Reference>,
    /// Quantos anos inteiros o período longo cobre (2 a 5), ou 0 sem ele.
    pub years: u32,
}

/// Os pagamentos com data ex em `[ex_date − days, ex_date)`.
fn window(history: &[Payment], ex_date: Date, days: i64) -> Vec<Payment> {
    let start = ex_date.saturating_sub(Duration::days(days));
    history
        .iter()
        .filter(|p| p.ex_date >= start && p.ex_date < ex_date)
        .copied()
        .collect()
}

/// Se o pagamento de `value` por cota com data ex `ex_date` está fora do
/// padrão de `history` (os pagamentos do ativo, por cota de hoje), com o
/// limiar `ratio` ([`ratio_for`]).
pub fn unusual(
    value: Decimal,
    ex_date: Date,
    history: &[Payment],
    ratio: Decimal,
) -> Option<Unusual> {
    let recent_window = window(history, ex_date, TRAILING_DAYS);
    let (recent_usual, _) = usual_of(value, &recent_window)?;
    let found = direction(value, recent_usual, ratio)?;
    let recent = reference(value, &recent_window)?;
    let first = history.iter().map(|p| p.ex_date).min()?;
    let span = (ex_date - first).whole_days() / TRAILING_DAYS;
    let years = u32::try_from(span).unwrap_or(0).min(USUAL_YEARS);
    let long = if years >= MIN_USUAL_YEARS {
        let long_window = window(history, ex_date, TRAILING_DAYS * i64::from(years));
        let (long_usual, _) = usual_of(value, &long_window)?;
        if direction(value, long_usual, ratio) != Some(found) {
            return None;
        }
        Some(reference(value, &long_window)?)
    } else {
        None
    };
    Some(Unusual {
        direction: found,
        per_unit: value,
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

    /// Um pagamento por mês, no dia 1, de `start` até antes de `end`.
    fn monthly(start: Date, end: Date, value: Decimal) -> Vec<Payment> {
        let mut out = Vec::new();
        let mut day = start;
        while day < end {
            out.push(Payment {
                ex_date: day,
                per_unit: value,
            });
            let (y, m) = if day.month() == time::Month::December {
                (day.year() + 1, time::Month::January)
            } else {
                (day.year(), day.month().next())
            };
            day = Date::from_calendar_date(y, m, 1).unwrap_or(end);
        }
        out
    }

    const EX: Date = date!(2026 - 12 - 01);

    #[test]
    fn a_monthly_fund_cut_by_a_quarter_is_out_of_pattern() {
        let history = monthly(date!(2020 - 01 - 01), EX, dec!(0.80));
        let found = unusual(dec!(0.60), EX, &history, FUND_RATIO).unwrap();
        assert_eq!(found.direction, Direction::Cut);
        assert_eq!(found.recent.usual, dec!(0.80));
        assert_eq!(found.recent.change, dec!(-0.25));
        assert_eq!(found.recent.payments, 12);
        assert_eq!(found.years, 5);
        assert_eq!(found.long.unwrap().change, dec!(-0.25));
        // Um centavo a menos de 20% não é corte; −20% é.
        assert!(unusual(dec!(0.65), EX, &history, FUND_RATIO).is_none());
        assert!(unusual(dec!(0.64), EX, &history, FUND_RATIO).is_some());
        // Na ação, o limiar é 1,5: −25% fica dentro do habitual.
        assert!(unusual(dec!(0.60), EX, &history, STOCK_RATIO).is_none());
    }

    #[test]
    fn a_bank_with_monthly_jcp_and_semiannual_complements_is_habitual() {
        // JCP mensal de 0,018 e complementos de 0,35 em março e junho de cada ano.
        let mut history = monthly(date!(2021 - 01 - 01), EX, dec!(0.018));
        for year in 2021..=2026 {
            for month in [time::Month::March, time::Month::June] {
                history.push(Payment {
                    ex_date: Date::from_calendar_date(year, month, 20).unwrap(),
                    per_unit: dec!(0.35),
                });
            }
        }
        history.sort_by_key(|p| p.ex_date);
        assert!(unusual(dec!(0.0182), EX, &history, STOCK_RATIO).is_none());
        assert!(unusual(dec!(0.36), EX, &history, STOCK_RATIO).is_none());
        // O complemento que cai para 0,12 não tem parecido: o mais próximo é 0,35.
        let cut = unusual(dec!(0.12), EX, &history, STOCK_RATIO).unwrap();
        assert_eq!(cut.direction, Direction::Cut);
        assert_eq!(cut.recent.usual, dec!(0.35));
        assert_eq!(cut.recent.payments, 1);
        // O JCP mensal que dobra é aumento.
        let up = unusual(dec!(0.03), EX, &history, STOCK_RATIO).unwrap();
        assert_eq!(up.direction, Direction::Increase);
    }

    #[test]
    fn both_periods_must_agree() {
        // Cinco anos de 1,00 e um ano de 0,70: voltar a 1,00 não é aumento (é o
        // habitual de 5 anos), e 0,70 não é corte nos 12 meses.
        let mut history = monthly(date!(2020 - 01 - 01), date!(2025 - 12 - 01), dec!(1.00));
        history.extend(monthly(date!(2025 - 12 - 01), EX, dec!(0.70)));
        assert!(unusual(dec!(1.00), EX, &history, FUND_RATIO).is_none());
        assert!(unusual(dec!(0.70), EX, &history, FUND_RATIO).is_none());
    }

    #[test]
    fn short_history_uses_the_twelve_months_alone() {
        let history = monthly(date!(2026 - 03 - 01), EX, dec!(0.10));
        let found = unusual(dec!(0.20), EX, &history, FUND_RATIO).unwrap();
        assert_eq!(found.direction, Direction::Increase);
        assert!(found.long.is_none());
        assert_eq!(found.years, 0);
        // Sem nada antes da data ex: sem habitual, sem marca.
        assert!(unusual(dec!(0.20), date!(2026 - 01 - 01), &history, FUND_RATIO).is_none());
        assert!(unusual(dec!(0), EX, &history, FUND_RATIO).is_none());
    }

    #[test]
    fn the_same_date_is_one_payment_and_b3_wins_over_yahoo() {
        let rows = [
            UnitPayment {
                ex_date: date!(2026 - 09 - 02),
                gross: dec!(0.10),
                net: dec!(0.0825),
            },
            UnitPayment {
                ex_date: date!(2026 - 09 - 02),
                gross: dec!(0.03),
                net: dec!(0.03),
            },
            UnitPayment {
                ex_date: date!(2026 - 06 - 02),
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
    }

    fn history_strategy() -> impl Strategy<Value = Vec<Payment>> {
        proptest::collection::vec((0i64..2200, 1i64..100_000), 1..40).prop_map(|points| {
            let mut list: Vec<Payment> = points
                .into_iter()
                .map(|(back, cents)| Payment {
                    ex_date: EX.saturating_sub(Duration::days(back + 1)),
                    per_unit: Decimal::new(cents, 4),
                })
                .collect();
            list.sort_by_key(|p| p.ex_date);
            list
        })
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
            let a = unusual(value, EX, &history, FUND_RATIO).map(|u| u.direction);
            let b = unusual(value * factor, EX, &scaled, FUND_RATIO).map(|u| u.direction);
            prop_assert_eq!(a, b);
        }

        /// O sentido diz o sinal da mudança, e a mudança passa do limiar.
        #[test]
        fn the_direction_matches_the_change(
            history in history_strategy(),
            cents in 1i64..100_000,
        ) {
            let value = Decimal::new(cents, 4);
            if let Some(found) = unusual(value, EX, &history, STOCK_RATIO) {
                // A mudança é arredondada a 4 casas: a folga é de uma casa.
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

        /// O habitual nunca sai de dentro do intervalo dos pagamentos.
        #[test]
        fn the_usual_is_between_the_payments(
            history in history_strategy(),
            cents in 1i64..100_000,
        ) {
            let value = Decimal::new(cents, 4);
            if let Some(found) = reference(value, &history) {
                let min = history.iter().map(|p| p.per_unit).min().unwrap_or_default();
                let max = history.iter().map(|p| p.per_unit).max().unwrap_or_default();
                prop_assert!(found.usual >= min.round_dp(8) && found.usual <= max.round_dp(8));
            }
        }
    }
}
