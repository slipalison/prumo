//! Fechamento ruim da fonte: um trecho curto que salta para longe e volta ao
//! nível de antes, sem desdobramento que explique. Regra pura: sem banco, sem
//! rede. Vale para todo preço que entra em `price_history` (desempenho, risco,
//! comparador); explicação e exemplos em `docs/risco.md` ("Fechamento ruim").
//!
//! O caso que motivou: um FII com fechamento de 1,07 em dois pregões entre
//! 110,13 e 109,58 — a fonte errou a escala em 100×, e a volatilidade do ativo
//! ia a 10.000% ao ano. Um desdobramento de verdade não volta; uma queda forte
//! de verdade também não. O que volta ao nível de antes em poucos pregões, depois
//! de um salto desse tamanho, é erro de dado.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use time::macros::{offset, time};
use time::{Date, Duration, OffsetDateTime, Time, UtcOffset};

/// O salto que abre o trecho suspeito: o fechamento chega ao dobro (ou mais)
/// ou à metade (ou menos) do último fechamento bom. Quedas fortes de verdade
/// (20% a 40% num pregão) ficam abaixo disso.
pub const SPIKE_FACTOR: f64 = 2.0;
/// A volta: o primeiro fechamento depois do trecho fica a até 20% (para mais
/// ou para menos) do último fechamento bom de antes dele.
pub const RETURN_FACTOR: f64 = 1.2;
/// O trecho tem de 1 a 5 pregões; mais longo que isso, é o preço de verdade.
pub const MAX_RUN: usize = 5;

/// O relógio do Prumo (Brasília, sem horário de verão desde 2019).
const BRASILIA: UtcOffset = offset!(-3);
/// A partir de quando, em Brasília, a barra de hoje de um ativo de bolsa já é o
/// fechamento: a B3 fecha às 17h ou às 18h (acompanha o horário de verão dos
/// EUA), a bolsa americana às 17h ou 18h de Brasília, mais o leilão de
/// fechamento e o atraso da fonte. É também o horário da tarefa diária.
pub const SETTLED_AT: Time = time!(19:00);
/// Quantos dias para trás a tarefa diária pede de novo e corrige o que a fonte
/// mudou (revisão de um fechamento recente): uns 5 pregões.
pub const REVISE_DAYS: Duration = Duration::days(7);

/// Que barra é: de bolsa (pregão com hora para fechar), de 24 horas (câmbio
/// `…=X`, cripto `…-USD`: o dia só fecha à meia-noite) ou o preço publicado do
/// Tesouro (`TD-…`, divulgado uma vez por dia, já definitivo).
fn round_the_clock(symbol: &str) -> bool {
    symbol.ends_with("=X") || symbol.ends_with("-USD")
}

/// Se o valor de `symbol` na data `date` já é o fechamento daquele dia, com o
/// relógio em `now`. Dia passado (em Brasília), sim; dia futuro, não; o dia de
/// hoje: o preço do Tesouro, sim (publicado e definitivo); câmbio e cripto,
/// nunca (o dia ainda corre); ativo de bolsa, só a partir de [`SETTLED_AT`].
/// Gravar a barra de um pregão aberto deixava o preço do meio do dia no lugar
/// do fechamento, para sempre (`docs/risco.md`).
pub fn settled(symbol: &str, date: Date, now: OffsetDateTime) -> bool {
    let local = now.to_offset(BRASILIA);
    let today = local.date();
    if date != today {
        return date < today;
    }
    if symbol.starts_with("TD-") {
        return true;
    }
    !round_the_clock(symbol) && local.time() >= SETTLED_AT
}

/// Desde quando um fechamento já guardado ainda pode ser corrigido pela fonte
/// (os últimos [`REVISE_DAYS`] até hoje, em Brasília). Antes disso o cache não
/// muda: guarda o preço como era.
pub fn revise_from(now: OffsetDateTime) -> Date {
    now.to_offset(BRASILIA).date() - REVISE_DAYS
}

fn ratio(value: Decimal, base: Decimal) -> Option<f64> {
    if base <= Decimal::ZERO || value <= Decimal::ZERO {
        return None;
    }
    (value / base).to_f64().filter(|r| r.is_finite())
}

fn far(value: Decimal, base: Decimal) -> bool {
    ratio(value, base).is_some_and(|r| r >= SPIKE_FACTOR || r <= 1.0 / SPIKE_FACTOR)
}

fn near(value: Decimal, base: Decimal) -> bool {
    ratio(value, base).is_some_and(|r| (1.0 / RETURN_FACTOR..=RETURN_FACTOR).contains(&r))
}

/// As datas dos fechamentos ruins de uma série (em ordem crescente de data): os
/// trechos de 1 a [`MAX_RUN`] pregões que ficam a [`SPIKE_FACTOR`] ou mais do
/// último fechamento bom e cujo pregão seguinte volta a até [`RETURN_FACTOR`]
/// dele, **sem** desdobramento, bonificação ou grupamento (`splits`, data do
/// evento) entre o fechamento bom de antes e o da volta. O trecho no fim da
/// série, que ainda não voltou, fica: só a volta prova o erro.
pub fn bad_closes(series: &[(Date, Decimal)], splits: &[(Date, Decimal)]) -> Vec<Date> {
    let mut bad = Vec::new();
    let mut i = 1;
    while i < series.len() {
        let (before_date, before) = series[i - 1];
        if far(series[i].1, before) {
            let mut end = i;
            while end < series.len() && end - i < MAX_RUN && far(series[end].1, before) {
                end += 1;
            }
            if let Some(&(back_date, back)) = series.get(end) {
                let explained = splits
                    .iter()
                    .any(|(date, _)| *date > before_date && *date <= back_date);
                if near(back, before) && !explained {
                    bad.extend(series[i..end].iter().map(|(date, _)| *date));
                    // O da volta é bom: a busca segue a partir dele.
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;
    use time::Duration;

    fn series(values: &[Decimal]) -> Vec<(Date, Decimal)> {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| (date!(2026 - 01 - 12) + Duration::days(i as i64), *v))
            .collect()
    }

    #[test]
    fn two_days_off_by_a_hundred_times_are_bad() {
        let s = series(&[
            dec!(110.13),
            dec!(1.07),
            dec!(1.06),
            dec!(109.58),
            dec!(110),
        ]);
        assert_eq!(bad_closes(&s, &[]), vec![s[1].0, s[2].0]);
        // Para cima também (um zero a mais).
        let up = series(&[dec!(10), dec!(1000), dec!(10.2)]);
        assert_eq!(bad_closes(&up, &[]), vec![up[1].0]);
    }

    #[test]
    fn real_split_and_real_crash_stay() {
        // Desdobro de 5 para 1 sem evento guardado: não volta, fica.
        let split = series(&[
            dec!(100),
            dec!(20),
            dec!(20.4),
            dec!(19.8),
            dec!(20.1),
            dec!(20),
        ]);
        assert!(bad_closes(&split, &[]).is_empty());
        // Queda forte de verdade (−36% e −60%) que não volta: fica.
        let crash = series(&[dec!(100), dec!(64), dec!(63), dec!(65)]);
        assert!(bad_closes(&crash, &[]).is_empty());
        let collapse = series(&[dec!(100), dec!(40), dec!(42), dec!(38), dec!(45)]);
        assert!(bad_closes(&collapse, &[]).is_empty());
    }

    #[test]
    fn a_split_event_explains_the_jump() {
        // Desdobro de 1 para 4 e grupamento de volta, com os dois eventos: não é
        // erro da fonte, por estranho que seja.
        let s = series(&[dec!(100), dec!(25), dec!(100)]);
        let splits = [(s[1].0, dec!(4)), (s[2].0, dec!(0.25))];
        assert!(bad_closes(&s, &splits).is_empty());
    }

    #[test]
    fn long_runs_and_open_ends_stay() {
        // Seis pregões longe e a volta: longo demais para ser erro.
        let mut values = vec![dec!(100)];
        values.extend([dec!(1); 6]);
        values.push(dec!(100));
        assert!(bad_closes(&series(&values), &[]).is_empty());
        // Cinco pregões e a volta: erro.
        let mut five = vec![dec!(100)];
        five.extend([dec!(1); 5]);
        five.push(dec!(101));
        assert_eq!(bad_closes(&series(&five), &[]).len(), 5);
        // No fim, ainda sem volta: fica até a volta chegar.
        assert!(bad_closes(&series(&[dec!(100), dec!(1)]), &[]).is_empty());
        // Volta longe do nível de antes (−30%): não é a volta de um erro.
        assert!(bad_closes(&series(&[dec!(100), dec!(1), dec!(70)]), &[]).is_empty());
        assert!(bad_closes(&[], &[]).is_empty());
    }

    #[test]
    fn todays_bar_waits_for_the_close() {
        use time::macros::datetime;
        let today = date!(2026 - 11 - 27);
        // 15h em Brasília (18h UTC): pregão aberto, nada de hoje.
        let afternoon = datetime!(2026-11-27 18:00 UTC);
        assert!(!settled("KNRI11.SA", today, afternoon));
        assert!(settled("KNRI11.SA", today - Duration::days(1), afternoon));
        // 19h em Brasília: o de bolsa já fechou; câmbio e cripto ainda não.
        let evening = datetime!(2026-11-27 22:00 UTC);
        assert!(settled("KNRI11.SA", today, evening));
        assert!(settled("^BVSP", today, evening));
        assert!(settled("AAPL", today, evening));
        assert!(!settled("USDBRL=X", today, evening));
        assert!(!settled("BTC-USD", today, evening));
        // O preço do Tesouro de hoje é o publicado.
        assert!(settled("TD-IPCA-20350515", today, afternoon));
        // 01h30 UTC de sábado ainda é sexta em Brasília: sexta fechou, sábado é futuro.
        let late = datetime!(2026-11-28 01:30 UTC);
        assert!(settled("USDBRL=X", today - Duration::days(1), late));
        assert!(settled("KNRI11.SA", today, late));
        assert!(!settled("KNRI11.SA", today + Duration::days(1), late));
    }

    proptest! {
        /// Série que anda menos que o salto de um pregão para o outro (até
        /// −45% ou +90% por dia) nunca perde fechamento.
        #[test]
        fn ordinary_moves_never_drop_a_close(moves in prop::collection::vec(-0.45f64..0.9, 1..60)) {
            let mut price = 100.0_f64;
            let mut values = vec![Decimal::from(100)];
            for m in moves {
                price *= 1.0 + m;
                values.push(Decimal::from_f64_retain(price).unwrap_or(Decimal::ONE));
            }
            prop_assert!(bad_closes(&series(&values), &[]).is_empty());
        }
    }
}
