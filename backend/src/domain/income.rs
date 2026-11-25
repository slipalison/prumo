//! Proventos e renda: regra pura, sem banco, sem rede e sem HTTP.
//!
//! O dinheiro que a carteira paga — dividendo, JCP, rendimento de FII,
//! amortização — vira um evento com data de pagamento. Daqui saem o valor
//! líquido, o status (recebido ou a receber, só pela data), a janela de 12
//! meses, a média mensal, a distribuição por pagador e a **estimativa** de renda
//! a partir do histórico por cota. A estimativa nunca se mistura com o que a
//! pessoa recebeu: são dois números, e o rótulo diz qual é qual.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use time::{Date, Duration, OffsetDateTime, UtcOffset};

use super::allocation::{percent, round_cents};

/// IR retido na fonte sobre o JCP: 15% do valor bruto.
pub const JCP_WITHHOLDING_RATE: Decimal = Decimal::from_parts(15, 0, 0, false, 2);

/// Quantos meses a janela de "renda dos últimos 12 meses" cobre.
pub const WINDOW_MONTHS: usize = 12;

/// Dias do histórico por cota que formam a estimativa de renda anual.
const TRAILING_DAYS: i64 = 365;

/// O tipo do provento. O nome vai para o banco e para o JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeKind {
    Dividend,
    Jcp,
    ReitIncome,
    Amortization,
    Other,
}

impl IncomeKind {
    pub const ALL: [IncomeKind; 5] = [
        IncomeKind::Dividend,
        IncomeKind::Jcp,
        IncomeKind::ReitIncome,
        IncomeKind::Amortization,
        IncomeKind::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            IncomeKind::Dividend => "dividend",
            IncomeKind::Jcp => "jcp",
            IncomeKind::ReitIncome => "reit_income",
            IncomeKind::Amortization => "amortization",
            IncomeKind::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        IncomeKind::ALL.into_iter().find(|k| k.as_str() == value)
    }
}

/// De onde o evento veio. `Estimate` existe no esquema para a projeção ser
/// gravável um dia; hoje a estimativa mora no cache de histórico por cota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeSource {
    Manual,
    Estimate,
    B3Import,
}

impl IncomeSource {
    pub const ALL: [IncomeSource; 3] = [
        IncomeSource::Manual,
        IncomeSource::Estimate,
        IncomeSource::B3Import,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            IncomeSource::Manual => "manual",
            IncomeSource::Estimate => "estimate",
            IncomeSource::B3Import => "b3_import",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        IncomeSource::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

/// O tipo de objetivo da pessoa (tabela `objectives`). Esta frente só usa
/// `MonthlyIncome`, a meta de renda mensal; os outros esperam quem os mostrar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveKind {
    NetWorth,
    MonthlyIncome,
    ClassValue,
}

impl ObjectiveKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectiveKind::NetWorth => "net_worth",
            ObjectiveKind::MonthlyIncome => "monthly_income",
            ObjectiveKind::ClassValue => "class_value",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            ObjectiveKind::NetWorth,
            ObjectiveKind::MonthlyIncome,
            ObjectiveKind::ClassValue,
        ]
        .into_iter()
        .find(|k| k.as_str() == value)
    }
}

/// Recebido (a data de pagamento já chegou) ou a receber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeStatus {
    Received,
    Upcoming,
}

impl IncomeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            IncomeStatus::Received => "received",
            IncomeStatus::Upcoming => "upcoming",
        }
    }
}

/// O dia de hoje no Brasil (UTC−3, sem horário de verão desde 2019). Os
/// proventos são datados no calendário da B3, não no de Greenwich: às 22h de
/// Brasília ainda é o mesmo dia.
pub fn today_in_brazil(now: OffsetDateTime) -> Date {
    brazil_offset().map_or_else(|| now.date(), |offset| now.to_offset(offset).date())
}

fn brazil_offset() -> Option<UtcOffset> {
    UtcOffset::from_hms(-3, 0, 0).ok()
}

/// Recebido quando a data de pagamento é hoje ou já passou.
pub fn status(pay_date: Date, today: Date) -> IncomeStatus {
    if pay_date <= today {
        IncomeStatus::Received
    } else {
        IncomeStatus::Upcoming
    }
}

/// Os três valores de um provento, sempre coerentes: `net = gross − withholding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncomeAmounts {
    pub gross: Decimal,
    pub withholding: Decimal,
    pub net: Decimal,
}

/// O IR retido na fonte que a regra do tipo manda: 15% no JCP, nada no resto
/// (dividendo e rendimento de FII são isentos para pessoa física).
pub fn default_withholding(kind: IncomeKind, gross: Decimal) -> Decimal {
    match kind {
        IncomeKind::Jcp => round_cents(gross * JCP_WITHHOLDING_RATE),
        _ => Decimal::ZERO,
    }
}

/// Por que um valor não vira provento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountsError {
    /// A retenção é maior que o bruto.
    WithholdingAboveGross,
}

/// Bruto = valor por cota × quantidade (em centavos). A retenção é a pedida
/// ou a da regra do tipo; não passa do bruto.
pub fn amounts(
    kind: IncomeKind,
    amount_per_unit: Decimal,
    quantity: Decimal,
    withholding: Option<Decimal>,
) -> Result<IncomeAmounts, AmountsError> {
    let gross = round_cents(amount_per_unit * quantity);
    let withholding = withholding.unwrap_or_else(|| default_withholding(kind, gross));
    if withholding < Decimal::ZERO || withholding > gross {
        return Err(AmountsError::WithholdingAboveGross);
    }
    Ok(IncomeAmounts {
        gross,
        withholding,
        net: gross - withholding,
    })
}

/// Um mês do calendário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YearMonth {
    pub year: i32,
    /// De 1 a 12.
    pub month: u8,
}

impl YearMonth {
    pub fn of(date: Date) -> Self {
        Self {
            year: date.year(),
            month: u8::from(date.month()),
        }
    }

    /// `n` meses depois (ou antes, se negativo).
    pub fn shift(self, n: i32) -> Self {
        let index = self.year * 12 + i32::from(self.month) - 1 + n;
        Self {
            year: index.div_euclid(12),
            month: u8::try_from(index.rem_euclid(12) + 1).unwrap_or(1),
        }
    }

    /// `2026-11`.
    pub fn label(self) -> String {
        format!("{:04}-{:02}", self.year, self.month)
    }
}

/// Uma linha do livro de proventos, só com o que a soma precisa.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub pay_date: Date,
    pub net: Decimal,
    /// Quem pagou: a chave agrupa (id do ativo ou o símbolo) e o rótulo é o
    /// que a tela mostra.
    pub payer_key: String,
    pub payer_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthTotals {
    pub month: YearMonth,
    pub received: Decimal,
    pub upcoming: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PayerShare {
    pub key: String,
    pub label: String,
    pub total: Decimal,
    /// Parte do total recebido nos 12 meses, de 0 a 100.
    pub percent: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YearRow {
    pub year: i32,
    /// Líquido (recebido e a receber) de janeiro a dezembro.
    pub months: [Decimal; 12],
    pub total: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncomeSummary {
    /// Os 12 meses terminando no mês de hoje, do mais velho ao atual.
    pub months: Vec<MonthTotals>,
    /// Líquido **recebido** nos 12 meses.
    pub total_12m: Decimal,
    /// `total_12m / 12`, em centavos.
    pub monthly_average: Decimal,
    /// Líquido que ainda vai cair (pagamento depois de hoje), de qualquer mês.
    pub upcoming_total: Decimal,
    pub by_payer: Vec<PayerShare>,
    /// Do ano mais novo para o mais velho.
    pub years: Vec<YearRow>,
}

/// Soma o livro: janela de 12 meses, média, pagadores e a tabela ano × mês.
pub fn summarize(entries: &[Entry], today: Date) -> IncomeSummary {
    let current = YearMonth::of(today);
    let first = current.shift(1 - i32::try_from(WINDOW_MONTHS).unwrap_or(12));
    let mut months: Vec<MonthTotals> = (0..WINDOW_MONTHS)
        .map(|i| MonthTotals {
            month: first.shift(i32::try_from(i).unwrap_or(0)),
            received: Decimal::ZERO,
            upcoming: Decimal::ZERO,
        })
        .collect();
    let mut payers: BTreeMap<String, (String, Decimal)> = BTreeMap::new();
    let mut years: BTreeMap<i32, [Decimal; 12]> = BTreeMap::new();
    let mut upcoming_total = Decimal::ZERO;

    for entry in entries {
        let month = YearMonth::of(entry.pay_date);
        let received = status(entry.pay_date, today) == IncomeStatus::Received;
        if !received {
            upcoming_total += entry.net;
        }
        if let Some(slot) = months.iter_mut().find(|m| m.month == month) {
            if received {
                slot.received += entry.net;
            } else {
                slot.upcoming += entry.net;
            }
            if received {
                payers
                    .entry(entry.payer_key.clone())
                    .or_insert_with(|| (entry.payer_label.clone(), Decimal::ZERO))
                    .1 += entry.net;
            }
        }
        let row = years.entry(month.year).or_insert([Decimal::ZERO; 12]);
        row[usize::from(month.month - 1)] += entry.net;
    }

    let total_12m: Decimal = months.iter().map(|m| m.received).sum();
    let mut by_payer: Vec<PayerShare> = payers
        .into_iter()
        .map(|(key, (label, total))| PayerShare {
            key,
            label,
            total,
            percent: percent(total, total_12m),
        })
        .collect();
    by_payer.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.label.cmp(&b.label)));

    IncomeSummary {
        months,
        total_12m,
        monthly_average: round_cents(total_12m / Decimal::from(WINDOW_MONTHS)),
        upcoming_total,
        by_payer,
        years: years
            .into_iter()
            .rev()
            .map(|(year, months)| YearRow {
                year,
                total: months.iter().sum(),
                months,
            })
            .collect(),
    }
}

/// Dividendo por cota num dia, do histórico do provedor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DividendPoint {
    /// Data ex (a data "com" é o dia anterior).
    pub ex_date: Date,
    pub amount: Decimal,
}

/// Quanto a cota pagou nos últimos 365 dias (por data ex, bruto): a soma dos
/// pontos em `(hoje − 365 dias, hoje]`.
pub fn trailing_per_unit(points: &[DividendPoint], today: Date) -> Decimal {
    let since = today.saturating_sub(Duration::days(TRAILING_DAYS));
    points
        .iter()
        .filter(|p| p.ex_date > since && p.ex_date <= today)
        .map(|p| p.amount)
        .sum()
}

/// A renda mensal que `quantity` cotas rendem se pagarem o que a cota pagou em
/// 12 meses, em centavos.
pub fn monthly_estimate(per_unit_12m: Decimal, quantity: Decimal) -> Decimal {
    round_cents(per_unit_12m * quantity / Decimal::from(WINDOW_MONTHS))
}

/// De onde sai o número da meta de renda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressBasis {
    /// A média mensal do que foi recebido e está no livro.
    Received,
    /// A estimativa a partir do histórico por cota: só entra quando o livro
    /// ainda não tem nada recebido nos 12 meses.
    Estimate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoalProgress {
    pub current_monthly: Decimal,
    pub basis: ProgressBasis,
    /// De 0 a 100 ou mais: passar da meta não é cortado.
    pub percent: Decimal,
    /// Só o recebido marca a meta como atingida; estimativa não vale.
    pub reached: bool,
}

/// O quanto da meta de renda mensal já está feito. O dado do livro sempre
/// vence a estimativa.
pub fn goal_progress(
    target: Decimal,
    received_average: Decimal,
    estimated: Decimal,
) -> GoalProgress {
    let (current_monthly, basis) = if received_average > Decimal::ZERO {
        (received_average, ProgressBasis::Received)
    } else {
        (estimated, ProgressBasis::Estimate)
    };
    GoalProgress {
        current_monthly,
        basis,
        percent: percent(current_monthly, target),
        reached: basis == ProgressBasis::Received
            && target > Decimal::ZERO
            && current_monthly >= target,
    }
}

/// A data a partir da qual os proventos entram na sugestão de caixa do aporte:
/// o dia (de Brasília) do último aporte registrado. Pagamento no próprio dia do
/// aporte não conta; só os dos dias seguintes.
pub fn cash_cutoff(last_contribution_at: OffsetDateTime) -> Date {
    today_in_brazil(last_contribution_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::{date, datetime};

    fn entry(pay_date: Date, net: Decimal, payer: &str) -> Entry {
        Entry {
            pay_date,
            net,
            payer_key: payer.to_owned(),
            payer_label: payer.to_owned(),
        }
    }

    #[test]
    fn jcp_is_taxed_fifteen_percent_at_source() {
        let a = amounts(IncomeKind::Jcp, dec!(0.5), dec!(1000), None).unwrap();
        assert_eq!(a.gross, dec!(500.00));
        assert_eq!(a.withholding, dec!(75.00));
        assert_eq!(a.net, dec!(425.00));
    }

    #[test]
    fn dividends_and_reit_income_have_no_withholding() {
        for kind in [
            IncomeKind::Dividend,
            IncomeKind::ReitIncome,
            IncomeKind::Amortization,
            IncomeKind::Other,
        ] {
            let a = amounts(kind, dec!(1.1), dec!(10), None).unwrap();
            assert_eq!(
                (a.gross, a.withholding, a.net),
                (dec!(11.00), dec!(0), dec!(11.00))
            );
        }
    }

    #[test]
    fn explicit_withholding_replaces_the_rule_but_not_past_the_gross() {
        let a = amounts(IncomeKind::Dividend, dec!(2), dec!(10), Some(dec!(6))).unwrap();
        assert_eq!(
            (a.gross, a.withholding, a.net),
            (dec!(20.00), dec!(6), dec!(14.00))
        );
        assert_eq!(
            amounts(IncomeKind::Dividend, dec!(2), dec!(10), Some(dec!(20.01))),
            Err(AmountsError::WithholdingAboveGross)
        );
        assert_eq!(
            amounts(IncomeKind::Dividend, dec!(2), dec!(10), Some(dec!(-1))),
            Err(AmountsError::WithholdingAboveGross)
        );
        let all = amounts(IncomeKind::Dividend, dec!(2), dec!(10), Some(dec!(20))).unwrap();
        assert_eq!(all.net, dec!(0));
    }

    #[test]
    fn gross_rounds_to_cents() {
        let a = amounts(IncomeKind::ReitIncome, dec!(0.9333), dec!(7), None).unwrap();
        assert_eq!(a.gross, dec!(6.53));
    }

    #[test]
    fn status_follows_the_pay_date() {
        let today = date!(2026 - 11 - 25);
        assert_eq!(status(date!(2026 - 11 - 24), today), IncomeStatus::Received);
        assert_eq!(status(today, today), IncomeStatus::Received);
        assert_eq!(status(date!(2026 - 11 - 26), today), IncomeStatus::Upcoming);
        assert_eq!(IncomeStatus::Upcoming.as_str(), "upcoming");
        assert_eq!(IncomeStatus::Received.as_str(), "received");
    }

    #[test]
    fn today_uses_the_brazilian_calendar() {
        // 01h UTC de 26/11 ainda é noite de 25/11 em Brasília.
        assert_eq!(
            today_in_brazil(datetime!(2026-11-26 01:00 UTC)),
            date!(2026 - 11 - 25)
        );
        assert_eq!(
            today_in_brazil(datetime!(2026-11-26 03:00 UTC)),
            date!(2026 - 11 - 26)
        );
    }

    #[test]
    fn months_shift_across_years() {
        let november = YearMonth {
            year: 2026,
            month: 11,
        };
        assert_eq!(november.shift(1).label(), "2026-12");
        assert_eq!(november.shift(2).label(), "2027-01");
        assert_eq!(november.shift(-11).label(), "2025-12");
        assert_eq!(november.shift(-12).label(), "2025-11");
        assert_eq!(
            YearMonth {
                year: 2026,
                month: 1
            }
            .shift(-1)
            .label(),
            "2025-12"
        );
    }

    #[test]
    fn kinds_and_sources_round_trip_through_their_wire_names() {
        for kind in IncomeKind::ALL {
            assert_eq!(IncomeKind::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{}\"", kind.as_str())
            );
        }
        for source in IncomeSource::ALL {
            assert_eq!(IncomeSource::parse(source.as_str()), Some(source));
        }
        assert_eq!(IncomeKind::parse("juros"), None);
        for kind in [
            ObjectiveKind::NetWorth,
            ObjectiveKind::MonthlyIncome,
            ObjectiveKind::ClassValue,
        ] {
            assert_eq!(ObjectiveKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ObjectiveKind::parse("goals"), None);
        assert_eq!(IncomeSource::parse("planilha"), None);
    }

    #[test]
    fn summary_covers_twelve_months_ending_today() {
        let today = date!(2026 - 11 - 25);
        let summary = summarize(&[], today);
        assert_eq!(summary.months.len(), 12);
        assert_eq!(summary.months[0].month.label(), "2025-12");
        assert_eq!(summary.months[11].month.label(), "2026-11");
        assert_eq!(summary.total_12m, dec!(0));
        assert_eq!(summary.monthly_average, dec!(0));
        assert!(summary.by_payer.is_empty());
        assert!(summary.years.is_empty());
    }

    #[test]
    fn summary_splits_received_from_upcoming_and_averages_twelve_months() {
        let today = date!(2026 - 11 - 25);
        let entries = [
            entry(date!(2026 - 11 - 14), dec!(100.00), "KNRI11"),
            entry(date!(2026 - 11 - 30), dec!(40.00), "KNRI11"),
            entry(date!(2026 - 12 - 15), dec!(60.00), "HGLG11"),
            entry(date!(2026 - 03 - 10), dec!(200.00), "ITSA4"),
            entry(date!(2025 - 12 - 01), dec!(140.00), "ITSA4"),
            // Fora da janela: novembro de 2025.
            entry(date!(2025 - 11 - 28), dec!(999.00), "ITSA4"),
        ];
        let summary = summarize(&entries, today);
        assert_eq!(summary.total_12m, dec!(440.00));
        assert_eq!(summary.monthly_average, dec!(36.67));
        assert_eq!(summary.upcoming_total, dec!(100.00));
        let november = &summary.months[11];
        assert_eq!(
            (november.received, november.upcoming),
            (dec!(100.00), dec!(40.00))
        );
        // A janela só tem 11/2026 como mês corrente: dezembro à frente não entra.
        assert!(summary
            .months
            .iter()
            .all(|m| m.month <= YearMonth::of(today)));

        let payers: Vec<(&str, Decimal)> = summary
            .by_payer
            .iter()
            .map(|p| (p.label.as_str(), p.total))
            .collect();
        assert_eq!(payers, [("ITSA4", dec!(340.00)), ("KNRI11", dec!(100.00))]);
        assert_eq!(summary.by_payer[0].percent, dec!(77.27));
    }

    #[test]
    fn year_table_has_every_event_in_its_month() {
        let today = date!(2026 - 11 - 25);
        let entries = [
            entry(date!(2026 - 01 - 10), dec!(10.00), "A"),
            entry(date!(2026 - 12 - 10), dec!(5.00), "A"),
            entry(date!(2025 - 01 - 10), dec!(7.00), "A"),
        ];
        let summary = summarize(&entries, today);
        let years: Vec<i32> = summary.years.iter().map(|y| y.year).collect();
        assert_eq!(years, [2026, 2025]);
        assert_eq!(summary.years[0].months[0], dec!(10.00));
        assert_eq!(summary.years[0].months[11], dec!(5.00));
        assert_eq!(summary.years[0].total, dec!(15.00));
        assert_eq!(summary.years[1].total, dec!(7.00));
    }

    #[test]
    fn trailing_window_is_a_year_ending_today() {
        let today = date!(2026 - 11 - 25);
        let points = [
            DividendPoint {
                ex_date: date!(2026 - 11 - 25),
                amount: dec!(1.00),
            },
            DividendPoint {
                ex_date: date!(2026 - 06 - 01),
                amount: dec!(0.50),
            },
            // 365 dias antes de hoje: o início da janela é aberto.
            DividendPoint {
                ex_date: date!(2025 - 11 - 25),
                amount: dec!(9.00),
            },
            DividendPoint {
                ex_date: date!(2025 - 11 - 26),
                amount: dec!(0.25),
            },
            // Futuro: ainda não aconteceu.
            DividendPoint {
                ex_date: date!(2026 - 11 - 26),
                amount: dec!(7.00),
            },
        ];
        assert_eq!(trailing_per_unit(&points, today), dec!(1.75));
        assert_eq!(trailing_per_unit(&[], today), dec!(0));
    }

    #[test]
    fn estimate_is_a_twelfth_of_the_year_times_the_quantity() {
        assert_eq!(monthly_estimate(dec!(13.2), dec!(10)), dec!(11.00));
        assert_eq!(monthly_estimate(dec!(1), dec!(100)), dec!(8.33));
        assert_eq!(monthly_estimate(dec!(0), dec!(100)), dec!(0));
        assert_eq!(monthly_estimate(dec!(5), dec!(0)), dec!(0));
    }

    #[test]
    fn goal_progress_prefers_what_was_received() {
        let p = goal_progress(dec!(1000), dec!(250), dec!(900));
        assert_eq!(p.basis, ProgressBasis::Received);
        assert_eq!(
            (p.current_monthly, p.percent, p.reached),
            (dec!(250), dec!(25.00), false)
        );

        let p = goal_progress(dec!(1000), dec!(0), dec!(900));
        assert_eq!(p.basis, ProgressBasis::Estimate);
        assert_eq!((p.current_monthly, p.percent), (dec!(900), dec!(90.00)));
    }

    #[test]
    fn only_received_income_reaches_the_goal() {
        assert!(goal_progress(dec!(1000), dec!(1000), dec!(0)).reached);
        assert!(goal_progress(dec!(1000), dec!(1500), dec!(0)).reached);
        assert_eq!(
            goal_progress(dec!(1000), dec!(1500), dec!(0)).percent,
            dec!(150.00)
        );
        assert!(!goal_progress(dec!(1000), dec!(0), dec!(5000)).reached);
        assert!(!goal_progress(dec!(0), dec!(0), dec!(0)).reached);
    }

    #[test]
    fn cash_starts_the_day_after_the_last_contribution_in_brazil() {
        // 23h de Brasília do dia 24 já é dia 25 em UTC.
        assert_eq!(
            cash_cutoff(datetime!(2026-11-25 02:00 UTC)),
            date!(2026 - 11 - 24)
        );
        assert_eq!(
            cash_cutoff(datetime!(2026-11-25 15:00 UTC)),
            date!(2026 - 11 - 25)
        );
    }

    proptest! {
        /// A retenção nunca passa do bruto e líquido + retenção fecha o bruto.
        #[test]
        fn net_plus_withholding_is_the_gross(
            cents in 0i64..10_000_000_000,
            kind in 0usize..5,
        ) {
            let kind = IncomeKind::ALL[kind];
            let per_unit = Decimal::new(cents, 2);
            let a = amounts(kind, per_unit, Decimal::ONE, None).unwrap();
            prop_assert_eq!(a.net + a.withholding, a.gross);
            prop_assert!(a.withholding <= a.gross);
            prop_assert!(a.net >= Decimal::ZERO);
            if kind == IncomeKind::Jcp {
                prop_assert!(a.withholding * dec!(100) <= a.gross * dec!(15) + dec!(0.5));
            } else {
                prop_assert_eq!(a.withholding, Decimal::ZERO);
            }
        }

        /// A soma dos 12 meses é o total recebido, e cada pagador soma o mesmo.
        #[test]
        fn window_totals_agree(
            offsets in proptest::collection::vec(-500i64..200, 0..40),
            cents in proptest::collection::vec(1i64..1_000_000, 40),
        ) {
            let today = date!(2026 - 11 - 25);
            let entries: Vec<Entry> = offsets
                .iter()
                .zip(&cents)
                .enumerate()
                .map(|(i, (offset, cents))| {
                    entry(
                        today.saturating_add(Duration::days(*offset)),
                        Decimal::new(*cents, 2),
                        &format!("P{}", i % 4),
                    )
                })
                .collect();
            let summary = summarize(&entries, today);
            prop_assert_eq!(summary.months.len(), 12);
            let received: Decimal = summary.months.iter().map(|m| m.received).sum();
            prop_assert_eq!(summary.total_12m, received);
            let by_payer: Decimal = summary.by_payer.iter().map(|p| p.total).sum();
            prop_assert_eq!(by_payer, received);
            let upcoming: Decimal = entries
                .iter()
                .filter(|e| e.pay_date > today)
                .map(|e| e.net)
                .sum();
            prop_assert_eq!(summary.upcoming_total, upcoming);
            let every: Decimal = entries.iter().map(|e| e.net).sum();
            let years: Decimal = summary.years.iter().map(|y| y.total).sum();
            prop_assert_eq!(years, every);
        }
    }
}
