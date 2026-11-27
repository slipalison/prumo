//! Dias úteis para o vencimento do DARF, com tabela versionada por ano, e os
//! dias de pregão da B3 ([`is_trading_day`]) para a data ex dos proventos.
//!
//! O imposto do mês vence no último dia útil do mês seguinte (IN RFB
//! 1.585/2015, art. 56, par. 5; P&R 730). Dia útil aqui é dia com expediente
//! bancário: fora sábado, domingo, feriado nacional e os dias sem expediente
//! bancário no país (Carnaval, Corpus Christi e 31 de dezembro). Na dúvida o
//! calendário puxa o vencimento para antes, nunca para depois.
//!
//! A tabela foi conferida contra a Agenda Tributária da Receita (código 6015,
//! jan/2025 a nov/2026): o DARF de novembro de 2025 venceu em 30/12/2025, não
//! em 31/12. Ano fora da tabela é [`TaxError::NoCalendarForYear`].

use time::{Date, Duration, Weekday};

use super::{TaxError, YearMonth};

/// Os dias sem expediente de um ano (só os que caem em dia de semana importam,
/// mas a lista traz todos para conferir com a fonte).
struct YearCalendar {
    year: i32,
    /// (mês, dia, motivo)
    closed: &'static [(u8, u8, &'static str)],
}

const NEW_YEAR: &str = "Confraternizacao Universal (Lei 662/1949, red. Lei 10.607/2002)";
const CARNIVAL: &str = "Carnaval (ponto facultativo federal, sem expediente bancario)";
const GOOD_FRIDAY: &str = "Paixao de Cristo (Lei 9.093/1995, art. 2)";
const TIRADENTES: &str = "Tiradentes (Lei 662/1949, red. Lei 10.607/2002)";
const LABOUR_DAY: &str = "Dia do Trabalho (Lei 662/1949, red. Lei 10.607/2002)";
const CORPUS_CHRISTI: &str = "Corpus Christi (ponto facultativo federal, sem expediente bancario)";
const INDEPENDENCE: &str = "Independencia (Lei 662/1949, red. Lei 10.607/2002)";
const APARECIDA: &str = "Nossa Senhora Aparecida (Lei 6.802/1980)";
const ALL_SOULS: &str = "Finados (Lei 662/1949, red. Lei 10.607/2002)";
const REPUBLIC: &str = "Proclamacao da Republica (Lei 662/1949, red. Lei 10.607/2002)";
const BLACK_CONSCIOUSNESS: &str = "Consciencia Negra (Lei 14.759/2023)";
const CHRISTMAS: &str = "Natal (Lei 662/1949, red. Lei 10.607/2002)";
const LAST_DAY: &str =
    "31/12 sem expediente bancario (Agenda Tributaria RFB: DARF 6015 de nov/2025 venceu em 30/12/2025)";

const TABLE: &[YearCalendar] = &[
    YearCalendar {
        year: 2025,
        closed: &[
            (1, 1, NEW_YEAR),
            (3, 3, CARNIVAL),
            (3, 4, CARNIVAL),
            (4, 18, GOOD_FRIDAY),
            (4, 21, TIRADENTES),
            (5, 1, LABOUR_DAY),
            (6, 19, CORPUS_CHRISTI),
            (9, 7, INDEPENDENCE),
            (10, 12, APARECIDA),
            (11, 2, ALL_SOULS),
            (11, 15, REPUBLIC),
            (11, 20, BLACK_CONSCIOUSNESS),
            (12, 25, CHRISTMAS),
            (12, 31, LAST_DAY),
        ],
    },
    YearCalendar {
        year: 2026,
        closed: &[
            (1, 1, NEW_YEAR),
            (2, 16, CARNIVAL),
            (2, 17, CARNIVAL),
            (4, 3, GOOD_FRIDAY),
            (4, 21, TIRADENTES),
            (5, 1, LABOUR_DAY),
            (6, 4, CORPUS_CHRISTI),
            (9, 7, INDEPENDENCE),
            (10, 12, APARECIDA),
            (11, 2, ALL_SOULS),
            (11, 15, REPUBLIC),
            (11, 20, BLACK_CONSCIOUSNESS),
            (12, 25, CHRISTMAS),
            (12, 31, LAST_DAY),
        ],
    },
    // 2027 entra porque o imposto de dezembro de 2026 vence em janeiro de 2027.
    YearCalendar {
        year: 2027,
        closed: &[
            (1, 1, NEW_YEAR),
            (2, 8, CARNIVAL),
            (2, 9, CARNIVAL),
            (3, 26, GOOD_FRIDAY),
            (4, 21, TIRADENTES),
            (5, 1, LABOUR_DAY),
            (5, 27, CORPUS_CHRISTI),
            (9, 7, INDEPENDENCE),
            (10, 12, APARECIDA),
            (11, 2, ALL_SOULS),
            (11, 15, REPUBLIC),
            (11, 20, BLACK_CONSCIOUSNESS),
            (12, 25, CHRISTMAS),
            (12, 31, LAST_DAY),
        ],
    },
];

fn year_calendar(year: i32) -> Result<&'static YearCalendar, TaxError> {
    TABLE
        .iter()
        .find(|calendar| calendar.year == year)
        .ok_or(TaxError::NoCalendarForYear(year))
}

/// Os anos com calendário cadastrado.
pub fn covered_years() -> Vec<i32> {
    TABLE.iter().map(|calendar| calendar.year).collect()
}

/// Se a data é dia útil (com expediente bancário).
pub fn is_business_day(date: Date) -> Result<bool, TaxError> {
    let calendar = year_calendar(date.year())?;
    if matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday) {
        return Ok(false);
    }
    let (month, day) = (u8::from(date.month()), date.day());
    Ok(!calendar
        .closed
        .iter()
        .any(|&(m, d, _)| m == month && d == day))
}

/// A véspera de Natal: banco abre (meio expediente), a B3 não tem pregão.
const CHRISTMAS_EVE: (u8, u8) = (12, 24);

/// Se a B3 tem pregão na data. O calendário de pregões que a B3 publica é o
/// dos dias sem expediente bancário desta tabela (feriados nacionais, Carnaval,
/// Corpus Christi e 31/12) mais a véspera de Natal (24/12), em que o banco abre
/// e a bolsa não. A data ex de um provento é o pregão seguinte à data com
/// (`income_auto::ex_date_after`), e é esta a conta que ela usa. Ano fora da
/// tabela é [`TaxError::NoCalendarForYear`].
pub fn is_trading_day(date: Date) -> Result<bool, TaxError> {
    let eve = (u8::from(date.month()), date.day()) == CHRISTMAS_EVE;
    Ok(is_business_day(date)? && !eve)
}

/// O último dia útil do mês.
pub fn last_business_day(period: YearMonth) -> Result<Date, TaxError> {
    year_calendar(period.year)?;
    let first_of_next = first_day(period.next())?;
    let mut day = first_of_next - Duration::days(1);
    while !is_business_day(day)? {
        day -= Duration::days(1);
    }
    Ok(day)
}

/// Vencimento do DARF do imposto apurado em `period`: último dia útil do mês
/// seguinte.
pub fn darf_due_date(period: YearMonth) -> Result<Date, TaxError> {
    last_business_day(period.next())
}

fn first_day(period: YearMonth) -> Result<Date, TaxError> {
    Date::from_calendar_date(period.year, period.time_month(), 1)
        .map_err(|_| TaxError::NoCalendarForYear(period.year))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn ym(year: i32, month: u8) -> YearMonth {
        YearMonth { year, month }
    }

    #[test]
    fn weekends_and_holidays_are_not_business_days() {
        assert!(!is_business_day(date!(2026 - 11 - 21)).unwrap()); // sábado
        assert!(!is_business_day(date!(2026 - 11 - 20)).unwrap()); // Consciência Negra
        assert!(!is_business_day(date!(2026 - 2 - 17)).unwrap()); // Carnaval
        assert!(!is_business_day(date!(2025 - 12 - 31)).unwrap());
        assert!(is_business_day(date!(2026 - 11 - 25)).unwrap());
    }

    #[test]
    fn the_b3_has_no_session_on_christmas_eve_nor_on_the_last_day() {
        // O banco abre em 24/12; a B3, não. Em 31/12 nenhum dos dois.
        for year in covered_years() {
            let eve = Date::from_calendar_date(year, time::Month::December, 24).unwrap();
            let last = Date::from_calendar_date(year, time::Month::December, 31).unwrap();
            assert!(!is_trading_day(eve).unwrap(), "{eve}");
            assert!(!is_trading_day(last).unwrap(), "{last}");
            if !matches!(eve.weekday(), Weekday::Saturday | Weekday::Sunday) {
                assert!(is_business_day(eve).unwrap(), "{eve}");
            }
        }
        // Carnaval, Corpus Christi e feriado nacional: sem pregão.
        assert!(!is_trading_day(date!(2026 - 2 - 16)).unwrap());
        assert!(!is_trading_day(date!(2026 - 6 - 4)).unwrap());
        assert!(!is_trading_day(date!(2026 - 11 - 20)).unwrap());
        assert!(is_trading_day(date!(2026 - 12 - 23)).unwrap());
        assert_eq!(
            is_trading_day(date!(2028 - 12 - 22)),
            Err(TaxError::NoCalendarForYear(2028))
        );
    }

    #[test]
    fn year_without_calendar_is_an_error() {
        assert_eq!(
            is_business_day(date!(2028 - 1 - 3)),
            Err(TaxError::NoCalendarForYear(2028))
        );
        assert_eq!(
            darf_due_date(ym(2027, 12)),
            Err(TaxError::NoCalendarForYear(2028))
        );
        assert_eq!(covered_years(), vec![2025, 2026, 2027]);
    }

    /// Vencimentos do código 6015 publicados na Agenda Tributária da Receita
    /// (gov.br/receitafederal/pt-br/assuntos/agenda-tributaria, consulta em
    /// 25/11/2026). Cada linha: mês de apuração → vencimento.
    #[test]
    fn due_dates_match_the_receita_tax_calendar() {
        let published = [
            (ym(2024, 12), date!(2025 - 01 - 31)),
            (ym(2025, 1), date!(2025 - 02 - 28)),
            (ym(2025, 2), date!(2025 - 03 - 31)),
            (ym(2025, 3), date!(2025 - 04 - 30)),
            (ym(2025, 4), date!(2025 - 05 - 30)),
            (ym(2025, 5), date!(2025 - 06 - 30)),
            (ym(2025, 6), date!(2025 - 07 - 31)),
            (ym(2025, 7), date!(2025 - 08 - 29)),
            (ym(2025, 8), date!(2025 - 09 - 30)),
            (ym(2025, 9), date!(2025 - 10 - 31)),
            (ym(2025, 10), date!(2025 - 11 - 28)),
            (ym(2025, 11), date!(2025 - 12 - 30)),
            (ym(2025, 12), date!(2026 - 01 - 30)),
            (ym(2026, 1), date!(2026 - 02 - 27)),
            (ym(2026, 2), date!(2026 - 03 - 31)),
            (ym(2026, 3), date!(2026 - 04 - 30)),
            (ym(2026, 4), date!(2026 - 05 - 29)),
            (ym(2026, 5), date!(2026 - 06 - 30)),
            (ym(2026, 6), date!(2026 - 07 - 31)),
            (ym(2026, 7), date!(2026 - 08 - 31)),
            (ym(2026, 8), date!(2026 - 09 - 30)),
            (ym(2026, 9), date!(2026 - 10 - 30)),
            (ym(2026, 10), date!(2026 - 11 - 30)),
        ];
        for (period, due) in published {
            assert_eq!(darf_due_date(period).unwrap(), due, "{period:?}");
        }
    }

    #[test]
    fn december_due_dates_skip_the_31st() {
        // Ainda não publicado na Agenda: segue a mesma regra de 2025.
        assert_eq!(darf_due_date(ym(2026, 11)).unwrap(), date!(2026 - 12 - 30));
        assert_eq!(darf_due_date(ym(2026, 12)).unwrap(), date!(2027 - 01 - 29));
    }
}
