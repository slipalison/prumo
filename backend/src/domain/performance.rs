//! Rentabilidade da carteira pelo método de cotas (TWR) e a comparação com
//! índices. Regra pura: sem banco, sem rede. O método, as fontes e os limites
//! estão em `docs/desempenho.md`.
//!
//! A carteira é tratada como um fundo de uma pessoa só: começa com a cota
//! valendo 1 e o patrimônio do dia em cotas. Aporte compra cotas pelo valor do
//! dia e não mexe no valor da cota; só a variação de preço mexe. Por isso a
//! rentabilidade não depende de quanto nem de quando se aportou.

use rust_decimal::{Decimal, MathematicalOps, RoundingStrategy};
use serde::{Deserialize, Serialize};
use time::{Date, Duration, Month};
use uuid::Uuid;

use super::allocation::round_cents;

/// Valor da cota no primeiro dia da série.
pub const INITIAL_QUOTA: Decimal = Decimal::ONE;
/// Casas da cota e da quantidade de cotas.
const QUOTA_DP: u32 = 12;
const HUNDRED: Decimal = Decimal::ONE_HUNDRED;

/// A cota de um dia: quantas a carteira tem e quanto cada uma vale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota {
    pub quotas: Decimal,
    pub value: Decimal,
}

fn round_quota(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(QUOTA_DP, RoundingStrategy::MidpointNearestEven)
}

/// A cota do dia, a partir da anterior, do patrimônio no fechamento e do fluxo
/// do dia (aporte positivo, retirada negativa), sem custo de compra: o que
/// entrou foi comprado pelo preço do fechamento. Ver [`advance_day`].
pub fn advance(previous: Option<Quota>, market_value: Decimal, net_flow: Decimal) -> Quota {
    advance_day(previous, market_value, net_flow, Decimal::ZERO)
}

/// A cota do dia, como um fundo que emite cotas pelo valor do fechamento:
///
/// 1. o preço anda sobre as cotas que já existiam: a cota "antes dos
///    negócios" é o patrimônio de antes sem o que entrou ou saiu, com o que
///    entrou valendo o preço do fechamento
///    (`(patrimônio − fluxo − custo_da_compra) / cotas_de_ontem`);
/// 2. o dinheiro que entrou compra cotas por essa cota;
/// 3. o custo da compra — o que se pagou acima do fechamento, mais as taxas
///    (`purchase_cost` é o valor no fechamento do que se comprou menos o que
///    se pagou: negativo quando custou) — é de todo o dinheiro que ficou na
///    carteira depois dos negócios, o de antes e o que chegou:
///    `× patrimônio / (patrimônio − custo_da_compra)`.
///
/// Sem custo de compra (comprou pelo fechamento, ajuste, saldo inicial, dia
/// sem negócio) é a conta de sempre, `(patrimônio − fluxo) / cotas_de_ontem`.
/// A venda abaixo do fechamento (e a taxa dela) continua com as cotas de
/// antes, que são as que venderam: está no fluxo.
///
/// Antes, o custo da compra caía inteiro sobre as cotas de ontem: com uma base
/// pequena (uma cota) e uma compra 50 vezes maior paga 1,6% acima do
/// fechamento, a cota perdia 80% num dia. Agora pesa no máximo a fração do
/// custo sobre o que se comprou.
pub fn advance_day(
    previous: Option<Quota>,
    market_value: Decimal,
    net_flow: Decimal,
    purchase_cost: Decimal,
) -> Quota {
    let market_value = market_value.max(Decimal::ZERO);
    let value = match previous {
        None => INITIAL_QUOTA,
        // Sem cotas ontem (carteira vazia ou só com ativo ainda não comprado):
        // não houve rendimento a medir, a cota fica onde estava.
        Some(p) if p.quotas <= Decimal::ZERO => p.value,
        Some(p) => {
            // Perda total levaria a cota a zero ou abaixo; o piso mantém a
            // série definida.
            let floor = Decimal::new(1, QUOTA_DP);
            let before_trades = (market_value - net_flow - purchase_cost) / p.quotas;
            let shared = market_value - purchase_cost;
            let cost = if purchase_cost.is_zero() || shared <= Decimal::ZERO {
                Decimal::ONE
            } else {
                market_value / shared
            };
            round_quota(before_trades * cost).max(floor)
        }
    };
    Quota {
        quotas: round_quota(market_value / value),
        value,
    }
}

/// O custo das compras do dia: o valor no fechamento do que se comprou menos o
/// que se pagou, com as taxas (negativo quando pagou acima do fechamento,
/// positivo abaixo). Só compra de verdade conta: quantidade e dinheiro
/// positivos, num ativo cotado que tem preço hoje (desdobramento e bonificação
/// não são dinheiro; renda fixa por valor entra pelo valor; ativo sem preço
/// fica fora da carteira até ganhar um).
pub fn purchase_cost(current: &[Position], purchases: &[Purchase]) -> Decimal {
    let cost: Decimal = purchases
        .iter()
        .filter(|p| p.quantity > Decimal::ZERO && p.value > Decimal::ZERO)
        .filter_map(|p| {
            let now = current.iter().find(|c| c.asset_id == p.asset_id)?;
            let price = now
                .unit_price
                .filter(|price| !now.by_value && *price > Decimal::ZERO)?;
            Some(p.quantity * price - p.value)
        })
        .sum();
    round_cents(cost)
}

/// Um ativo na foto do dia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub asset_id: Uuid,
    /// Ativo por valor (renda fixa): `quantity` já é o valor em reais.
    pub by_value: bool,
    pub quantity: Decimal,
    /// Preço em reais. `None` = sem cotação naquele dia (o ativo vale 0).
    pub unit_price: Option<Decimal>,
}

impl Position {
    fn priced(&self) -> bool {
        self.by_value || self.unit_price.is_some_and(|p| p > Decimal::ZERO)
    }

    pub fn value(&self) -> Decimal {
        if self.by_value {
            self.quantity
        } else {
            self.quantity * self.unit_price.unwrap_or(Decimal::ZERO)
        }
    }
}

/// O patrimônio de uma foto, em centavos.
pub fn market_value(positions: &[Position]) -> Decimal {
    round_cents(positions.iter().map(Position::value).sum())
}

/// O que um aporte registrado comprou de um ativo entre duas fotos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Purchase {
    pub asset_id: Uuid,
    pub quantity: Decimal,
    /// O que a pessoa pagou, em reais.
    pub value: Decimal,
}

/// O fluxo entre duas fotos da carteira: o que entrou ou saiu de dinheiro,
/// e não rendimento.
///
/// - O aporte registrado entra pelo valor pago.
/// - Num ativo cotado, a quantidade que mudou sem aporte (edição à mão, ativo
///   novo, ativo apagado) entra pelo preço do dia: é compra ou venda feita
///   fora do Prumo, não ganho.
/// - Ativo que ganhou ou perdeu a cotação entra ou sai inteiro, para a falta de
///   preço não virar perda (nem a volta dele, ganho).
/// - Num ativo por valor (renda fixa) que já estava na carteira, a mudança de
///   valor sem aporte é o rendimento informado pela pessoa. Ativo por valor que
///   aparece ou some entra ou sai inteiro.
pub fn net_flow(previous: &[Position], current: &[Position], purchases: &[Purchase]) -> Decimal {
    let bought = |id: Uuid| {
        purchases
            .iter()
            .filter(|p| p.asset_id == id)
            .fold((Decimal::ZERO, Decimal::ZERO), |(q, v), p| {
                (q + p.quantity, v + p.value)
            })
    };
    let mut flow = Decimal::ZERO;
    for now in current {
        let before = previous.iter().find(|p| p.asset_id == now.asset_id);
        let (bought_quantity, bought_value) = bought(now.asset_id);
        flow += if now.by_value {
            match before {
                Some(before) if before.by_value => bought_value,
                Some(before) => now.quantity - before.value(),
                None => now.quantity,
            }
        } else if now.priced() {
            let held = before
                .filter(|b| b.priced() && !b.by_value)
                .map_or(Decimal::ZERO, |b| b.quantity);
            let outside = before
                .filter(|b| b.by_value)
                .map_or(Decimal::ZERO, Position::value);
            let price = now.unit_price.unwrap_or(Decimal::ZERO);
            bought_value + (now.quantity - held - bought_quantity) * price - outside
        } else {
            -before
                .filter(|b| b.priced())
                .map_or(Decimal::ZERO, Position::value)
        };
    }
    for gone in previous
        .iter()
        .filter(|p| p.priced() && !current.iter().any(|c| c.asset_id == p.asset_id))
    {
        flow -= gone.value();
    }
    round_cents(flow)
}

/// Uma cota numa data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaPoint {
    pub date: Date,
    pub value: Decimal,
}

/// A rentabilidade de uma janela e as datas que ela usou de fato.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Data da cota de base (o fechamento de onde se mede).
    pub from: Date,
    /// Data da última cota da janela.
    pub to: Date,
    /// Fração (0,05 = 5%).
    pub ret: Decimal,
}

/// A rentabilidade de `from` a `to`. A base é a última cota em ou antes de
/// `from`; se a série começa depois, a base é a primeira cota (a janela fica
/// mais curta, e `Window::from` diz de quando). `None` sem cota até `to`.
pub fn period_return(points: &[QuotaPoint], from: Date, to: Date) -> Option<Window> {
    let end = points.iter().rev().find(|p| p.date <= to)?;
    let base = points
        .iter()
        .rev()
        .find(|p| p.date <= from)
        .or_else(|| points.first())
        .filter(|b| b.date <= end.date)?;
    Some(Window {
        from: base.date,
        to: end.date,
        ret: ratio(end.value, base.value),
    })
}

fn ratio(end: Decimal, base: Decimal) -> Decimal {
    if base <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    end / base - Decimal::ONE
}

/// Os períodos da tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// Desde o fechamento do último dia do mês passado.
    Month,
    /// Desde o fechamento de 31/12 do ano passado.
    YearToDate,
    /// Doze meses até hoje.
    TwelveMonths,
    /// Dois anos até hoje.
    TwoYears,
    /// Cinco anos até hoje.
    FiveYears,
    /// Desde a primeira cota.
    All,
}

impl Period {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "month" => Some(Period::Month),
            "ytd" => Some(Period::YearToDate),
            "12m" => Some(Period::TwelveMonths),
            "2y" => Some(Period::TwoYears),
            "5y" => Some(Period::FiveYears),
            "all" => Some(Period::All),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Period::Month => "month",
            Period::YearToDate => "ytd",
            Period::TwelveMonths => "12m",
            Period::TwoYears => "2y",
            Period::FiveYears => "5y",
            Period::All => "all",
        }
    }

    /// A data de base do período que termina em `today`.
    pub fn start(self, today: Date, first: Date) -> Date {
        match self {
            Period::Month => today.replace_day(1).unwrap_or(today) - Duration::days(1),
            Period::YearToDate => {
                Date::from_calendar_date(today.year() - 1, Month::December, 31).unwrap_or(first)
            }
            Period::TwelveMonths => months_before(today, 12),
            Period::TwoYears => months_before(today, 24),
            Period::FiveYears => months_before(today, 60),
            Period::All => first,
        }
    }
}

/// A mesma data `months` meses antes (31/03 menos um mês = 28 ou 29/02).
pub fn months_before(date: Date, months: u32) -> Date {
    let total = date.year() * 12 + i32::from(u8::from(date.month())) - 1 - months as i32;
    let year = total.div_euclid(12);
    let month = Month::try_from(u8::try_from(total.rem_euclid(12) + 1).unwrap_or(1))
        .unwrap_or(Month::January);
    let last = time::util::days_in_month(month, year);
    Date::from_calendar_date(year, month, date.day().min(last)).unwrap_or(date)
}

/// Um índice acumulado numa janela e até quando havia dado publicado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accrued {
    /// Fração.
    pub ret: Decimal,
    /// Última data coberta por dado publicado. `None`: nenhum dado na janela.
    pub as_of: Option<Date>,
}

/// Taxa diária (CDI, Selic, em % ao dia) acumulada de um fechamento a outro: a
/// taxa da data `d` rende de `d` para o dia útil seguinte, então entram as
/// datas `from <= d < to`.
pub fn daily_rate_accrued(rates: &[(Date, Decimal)], from: Date, to: Date) -> Accrued {
    let mut factor = Decimal::ONE;
    let mut as_of = None;
    for (date, rate) in rates.iter().filter(|(d, _)| *d >= from && *d < to) {
        factor *= Decimal::ONE + rate / HUNDRED;
        as_of = Some(*date);
    }
    Accrued {
        ret: factor - Decimal::ONE,
        as_of: as_of.map(|d: Date| d.next_day().unwrap_or(d)),
    }
}

/// Variação mensal (IPCA, em % no mês, datada no dia 1º) acumulada de um
/// fechamento a outro. Mês pela metade entra pela fração de dias corridos, de
/// forma composta. Mês ainda não publicado fica de fora, e `as_of` diz até
/// onde foi.
pub fn monthly_rate_accrued(rates: &[(Date, Decimal)], from: Date, to: Date) -> Accrued {
    let mut factor = Decimal::ONE;
    let mut as_of = None;
    // A janela é (from, to]: o primeiro dia que conta é o seguinte a `from`.
    let first = from.next_day().unwrap_or(from);
    let mut month = first.replace_day(1).unwrap_or(first);
    while month <= to {
        let days = time::util::days_in_month(month.month(), month.year());
        let month_end = month.replace_day(days).unwrap_or(month);
        let Some((_, rate)) = rates.iter().find(|(d, _)| *d == month) else {
            break;
        };
        // Dias da janela (from, to] dentro do mês.
        let start = from.max(month - Duration::days(1));
        let end = to.min(month_end);
        let covered = (end - start).whole_days();
        if covered > 0 {
            let growth = Decimal::ONE + rate / HUNDRED;
            let share = Decimal::from(covered) / Decimal::from(days);
            factor *= if share == Decimal::ONE {
                growth
            } else {
                growth.checked_powd(share).unwrap_or(Decimal::ONE)
            };
            as_of = Some(end);
        }
        match month_end.next_day() {
            Some(next) => month = next,
            None => break,
        }
    }
    Accrued {
        ret: factor - Decimal::ONE,
        as_of,
    }
}

/// Índice de nível (Ibovespa, em pontos): o último fechamento em ou antes de
/// `to` sobre o último em ou antes de `from`.
pub fn level_accrued(levels: &[(Date, Decimal)], from: Date, to: Date) -> Accrued {
    let base = levels.iter().rev().find(|(d, _)| *d <= from);
    let end = levels.iter().rev().find(|(d, _)| *d <= to && *d >= from);
    match (base, end) {
        (Some((_, base)), Some((date, end))) => Accrued {
            ret: ratio(*end, *base),
            as_of: Some(*date),
        },
        _ => Accrued {
            ret: Decimal::ZERO,
            as_of: None,
        },
    }
}

/// A carteira contra um índice na mesma janela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Versus {
    /// Quanto a carteira rendeu a mais (ou a menos) que o índice, de forma
    /// composta: `(1 + r) / (1 + i) - 1`. É o "x% acima do CDI".
    pub excess: Decimal,
    /// A rentabilidade como porcentagem do índice (`r / i`, 1,1 = 110% do
    /// CDI). `None` quando o índice não subiu (a razão não diz nada).
    pub relative: Option<Decimal>,
}

pub fn versus(portfolio: Decimal, benchmark: Decimal) -> Versus {
    let base = Decimal::ONE + benchmark;
    Versus {
        excess: if base > Decimal::ZERO {
            (Decimal::ONE + portfolio) / base - Decimal::ONE
        } else {
            Decimal::ZERO
        },
        relative: (benchmark > Decimal::ZERO).then(|| portfolio / benchmark),
    }
}

/// Um mês da série: a rentabilidade do mês e a acumulada desde o começo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthCell {
    pub year: i32,
    pub month: Month,
    /// O fechamento de base: o último do mês anterior, ou a primeira cota no
    /// mês em que a série começa.
    pub from: Date,
    /// O último fechamento do mês.
    pub to: Date,
    /// Fração.
    pub ret: Decimal,
    /// Desde a primeira cota da série até `to`, fração.
    pub accumulated: Decimal,
}

/// Os meses da série, em ordem. Cada mês mede do último fechamento do mês
/// anterior (ou da primeira cota, no mês em que a série começa) ao último
/// fechamento do mês; mês com um fechamento só (o primeiro dia da série) não
/// tem o que medir e fica de fora.
pub fn monthly_cells(points: &[QuotaPoint]) -> Vec<MonthCell> {
    let Some(origin) = points.first() else {
        return Vec::new();
    };
    let mut cells = Vec::new();
    let mut base: Option<&QuotaPoint> = None;
    let mut index = 0;
    while index < points.len() {
        let first = &points[index];
        let (year, month) = (first.date.year(), first.date.month());
        let mut last = first;
        while index < points.len()
            && points[index].date.year() == year
            && points[index].date.month() == month
        {
            last = &points[index];
            index += 1;
        }
        let start = base.unwrap_or(first);
        if last.date > start.date {
            cells.push(MonthCell {
                year,
                month,
                from: start.date,
                to: last.date,
                ret: ratio(last.value, start.value),
                accumulated: ratio(last.value, origin.value),
            });
        }
        base = Some(last);
    }
    cells
}

/// Um ano da tabela mês × ano.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YearRow {
    pub year: i32,
    /// Janeiro a dezembro; `None` onde não há série.
    pub months: [Option<Decimal>; 12],
    /// O ano composto (só os meses com série).
    pub total: Decimal,
    /// Janela do ano: da base ao último fechamento.
    pub from: Date,
    pub to: Date,
}

/// A tabela mês × ano: os [`monthly_cells`] agrupados por ano, e o ano é a
/// composição dos meses.
pub fn monthly_table(points: &[QuotaPoint]) -> Vec<YearRow> {
    let mut rows: Vec<YearRow> = Vec::new();
    for cell in monthly_cells(points) {
        let row = match rows.last_mut() {
            Some(row) if row.year == cell.year => row,
            _ => {
                rows.push(YearRow {
                    year: cell.year,
                    months: [None; 12],
                    total: Decimal::ZERO,
                    from: cell.from,
                    to: cell.to,
                });
                let at = rows.len() - 1;
                &mut rows[at]
            }
        };
        row.months[month_index(cell.month)] = Some(cell.ret);
        row.total = (Decimal::ONE + row.total) * (Decimal::ONE + cell.ret) - Decimal::ONE;
        row.to = cell.to;
    }
    rows
}

/// Janeiro = 0.
pub fn month_index(month: Month) -> usize {
    usize::from(u8::from(month)) - 1
}

/// Fração → percentual (0 a 100), com `dp` casas.
pub fn to_percent(fraction: Decimal, dp: u32) -> Decimal {
    (fraction * HUNDRED).round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn units(id: Uuid, quantity: Decimal, price: Decimal) -> Position {
        Position {
            asset_id: id,
            by_value: false,
            quantity,
            unit_price: Some(price),
        }
    }

    fn by_value(id: Uuid, amount: Decimal) -> Position {
        Position {
            asset_id: id,
            by_value: true,
            quantity: amount,
            unit_price: None,
        }
    }

    #[test]
    fn first_day_starts_the_quota_at_one() {
        let q = advance(None, dec!(1000), dec!(1000));
        assert_eq!(q.value, dec!(1));
        assert_eq!(q.quotas, dec!(1000));
    }

    #[test]
    fn contribution_buys_quotas_without_moving_the_quota() {
        let day0 = advance(None, dec!(1000), dec!(1000));
        // Preço subiu 10% e entrou um aporte de 500 no fim do dia.
        let day1 = advance(Some(day0), dec!(1600), dec!(500));
        assert_eq!(day1.value, dec!(1.1));
        assert_eq!(day1.quotas.round_dp(6), dec!(1454.545455));
        // Sem o aporte, a mesma cota.
        let alone = advance(Some(day0), dec!(1100), Decimal::ZERO);
        assert_eq!(alone.value, day1.value);
    }

    /// Uma cota de base e uma compra 50 vezes maior paga 1,6% acima do
    /// fechamento (fictício; pela regra antiga a cota perdia 80% no dia): o
    /// custo é de todo o dinheiro do dia, e a cota perde ~1,6% por ele.
    #[test]
    fn purchase_cost_is_shared_by_all_the_money_of_the_day() {
        let id = Uuid::from_u128(7);
        let day0 = advance(None, dec!(80), dec!(80));
        let current = [units(id, dec!(51), dec!(80.40))];
        let bought = [Purchase {
            asset_id: id,
            quantity: dec!(50),
            value: dec!(4085),
        }];
        let cost = purchase_cost(&current, &bought);
        assert_eq!(cost, dec!(-65));
        let value = market_value(&current);
        let flow = net_flow(&[units(id, dec!(1), dec!(80))], &current, &bought);
        let day1 = advance_day(Some(day0), value, flow, cost);
        // O preço andou 80 → 80,40 sobre a cota de ontem; o custo de 65 pesa
        // sobre os 4.165,40 do dia.
        let expected = dec!(80.40) / dec!(80) * value / (value - cost);
        assert!(
            (day1.value - expected).abs() < dec!(0.000001),
            "{} != {expected}",
            day1.value
        );
        assert!(day1.value > dec!(0.98));
        // Sem custo (comprou pelo fechamento), a conta de sempre.
        let at_close = [Purchase {
            value: dec!(50) * dec!(80.40),
            ..bought[0]
        }];
        assert_eq!(purchase_cost(&current, &at_close), Decimal::ZERO);
        // Desdobramento e bonificação (sem dinheiro) e renda fixa não são compra.
        let split = [Purchase {
            asset_id: id,
            quantity: dec!(10),
            value: Decimal::ZERO,
        }];
        assert_eq!(purchase_cost(&current, &split), Decimal::ZERO);
        assert_eq!(
            purchase_cost(&[by_value(id, dec!(500))], &bought),
            Decimal::ZERO
        );
    }

    #[test]
    fn empty_portfolio_keeps_the_quota_until_money_arrives() {
        let day0 = advance(None, Decimal::ZERO, Decimal::ZERO);
        assert_eq!((day0.value, day0.quotas), (dec!(1), Decimal::ZERO));
        let day1 = advance(Some(day0), dec!(800), dec!(800));
        assert_eq!((day1.value, day1.quotas), (dec!(1), dec!(800)));
    }

    #[test]
    fn total_loss_keeps_a_positive_quota() {
        let day0 = advance(None, dec!(100), dec!(100));
        let day1 = advance(Some(day0), Decimal::ZERO, dec!(50));
        assert!(day1.value > Decimal::ZERO);
        assert_eq!(day1.quotas, Decimal::ZERO);
    }

    #[test]
    fn flows_come_from_contributions_and_from_quantity_changed_by_hand() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let before = [units(a, dec!(10), dec!(10)), by_value(b, dec!(1000))];
        // Preço de A foi a 11; comprou 5 de A por 10,50 no Prumo e mais 2 à mão;
        // a renda fixa rendeu 10 (informado) e recebeu 200 de aporte; entrou C.
        let after = [
            units(a, dec!(17), dec!(11)),
            by_value(b, dec!(1210)),
            units(c, dec!(3), dec!(20)),
        ];
        let bought = [
            Purchase {
                asset_id: a,
                quantity: dec!(5),
                value: dec!(52.50),
            },
            Purchase {
                asset_id: b,
                quantity: dec!(200),
                value: dec!(200),
            },
        ];
        // 52,50 + 2 × 11 + 200 + 3 × 20
        assert_eq!(net_flow(&before, &after, &bought), dec!(334.50));
    }

    #[test]
    fn removed_assets_and_lost_quotes_leave_at_the_last_value() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let before = [
            units(a, dec!(10), dec!(10)),
            by_value(b, dec!(500)),
            units(c, dec!(4), dec!(25)),
        ];
        let after = [Position {
            unit_price: None,
            ..units(c, dec!(4), dec!(0))
        }];
        assert_eq!(net_flow(&before, &after, &[]), dec!(-700));
        // E a cotação que volta entra inteira.
        assert_eq!(
            net_flow(&after, &[units(c, dec!(4), dec!(30))], &[]),
            dec!(120)
        );
    }

    #[test]
    fn asset_that_changes_between_value_and_units_counts_as_swap() {
        let a = Uuid::new_v4();
        let as_value = [by_value(a, dec!(300))];
        let as_units = [units(a, dec!(3), dec!(110))];
        assert_eq!(net_flow(&as_value, &as_units, &[]), dec!(30));
        assert_eq!(net_flow(&as_units, &as_value, &[]), dec!(-30));
    }

    fn points(values: &[(Date, Decimal)]) -> Vec<QuotaPoint> {
        values
            .iter()
            .map(|(date, value)| QuotaPoint {
                date: *date,
                value: *value,
            })
            .collect()
    }

    #[test]
    fn period_return_uses_the_last_close_before_the_window() {
        let series = points(&[
            (date!(2026 - 10 - 30), dec!(1.00)),
            (date!(2026 - 10 - 31), dec!(1.02)),
            (date!(2026 - 11 - 10), dec!(1.05)),
            (date!(2026 - 11 - 24), dec!(1.071)),
        ]);
        let month = period_return(&series, date!(2026 - 10 - 31), date!(2026 - 11 - 25)).unwrap();
        assert_eq!(month.from, date!(2026 - 10 - 31));
        assert_eq!(month.to, date!(2026 - 11 - 24));
        assert_eq!(month.ret, dec!(0.05));
        // Janela que começa antes da série: mede desde a primeira cota.
        let all = period_return(&series, date!(2020 - 01 - 01), date!(2026 - 11 - 25)).unwrap();
        assert_eq!(all.from, date!(2026 - 10 - 30));
        assert_eq!(all.ret, dec!(0.071));
        assert!(period_return(&series, date!(2020 - 01 - 01), date!(2026 - 01 - 01)).is_none());
        assert!(period_return(&[], date!(2020 - 01 - 01), date!(2026 - 01 - 01)).is_none());
    }

    #[test]
    fn periods_start_where_the_screen_says() {
        let today = date!(2026 - 03 - 31);
        let first = date!(2025 - 06 - 01);
        assert_eq!(Period::Month.start(today, first), date!(2026 - 02 - 28));
        assert_eq!(
            Period::YearToDate.start(today, first),
            date!(2025 - 12 - 31)
        );
        assert_eq!(
            Period::TwelveMonths.start(today, first),
            date!(2025 - 03 - 31)
        );
        assert_eq!(Period::All.start(today, first), first);
        assert_eq!(
            months_before(date!(2024 - 03 - 31), 1),
            date!(2024 - 02 - 29)
        );
        assert_eq!(
            months_before(date!(2026 - 01 - 15), 13),
            date!(2024 - 12 - 15)
        );
        assert_eq!(Period::TwoYears.start(today, first), date!(2024 - 03 - 31));
        assert_eq!(Period::FiveYears.start(today, first), date!(2021 - 03 - 31));
        for p in ["month", "ytd", "12m", "2y", "5y", "all"] {
            assert_eq!(Period::parse(p).map(Period::as_str), Some(p));
        }
        assert!(Period::parse("10y").is_none());
    }

    /// CDI de 0,050788% ao dia, como o BCB publicou em novembro de 2026.
    #[test]
    fn daily_rates_accrue_from_close_to_close() {
        let rate = dec!(0.050788);
        let rates = [
            (date!(2026 - 11 - 18), rate),
            (date!(2026 - 11 - 19), rate),
            (date!(2026 - 11 - 23), rate),
            (date!(2026 - 11 - 24), rate),
        ];
        let cdi = daily_rate_accrued(&rates, date!(2026 - 11 - 19), date!(2026 - 11 - 24));
        // 19 e 23 rendem até o fechamento de 24; a taxa de 24 rende para 25.
        let expected = (Decimal::ONE + rate / HUNDRED).powi(2) - Decimal::ONE;
        assert_eq!(cdi.ret, expected);
        assert_eq!(cdi.as_of, Some(date!(2026 - 11 - 24)));
        let none = daily_rate_accrued(&rates, date!(2026 - 11 - 25), date!(2026 - 11 - 30));
        assert_eq!((none.ret, none.as_of), (Decimal::ZERO, None));
    }

    #[test]
    fn monthly_index_accrues_whole_and_partial_months() {
        let ipca = [
            (date!(2026 - 09 - 01), dec!(0.82)),
            (date!(2026 - 10 - 01), dec!(0.14)),
        ];
        let whole = monthly_rate_accrued(&ipca, date!(2026 - 08 - 31), date!(2026 - 10 - 31));
        assert_eq!(
            whole.ret.round_dp(10),
            (dec!(1.0082) * dec!(1.0014) - Decimal::ONE).round_dp(10)
        );
        assert_eq!(whole.as_of, Some(date!(2026 - 10 - 31)));
        // Meio mês de setembro (15 de 30 dias) e novembro ainda não publicado.
        let partial = monthly_rate_accrued(&ipca, date!(2026 - 09 - 15), date!(2026 - 11 - 20));
        let half = dec!(1.0082).checked_powd(dec!(0.5)).unwrap();
        assert_eq!(
            partial.ret.round_dp(8),
            (half * dec!(1.0014) - Decimal::ONE).round_dp(8)
        );
        assert_eq!(partial.as_of, Some(date!(2026 - 10 - 31)));
        let empty = monthly_rate_accrued(&ipca, date!(2026 - 11 - 01), date!(2026 - 11 - 20));
        assert_eq!(empty.as_of, None);
    }

    #[test]
    fn level_index_compares_two_closes() {
        let ibov = [
            (date!(2026 - 11 - 17), dec!(169817)),
            (date!(2026 - 11 - 24), dec!(164090)),
        ];
        let r = level_accrued(&ibov, date!(2026 - 11 - 18), date!(2026 - 11 - 25));
        assert_eq!(r.ret, dec!(164090) / dec!(169817) - Decimal::ONE);
        assert_eq!(r.as_of, Some(date!(2026 - 11 - 24)));
        let before = level_accrued(&ibov, date!(2026 - 11 - 01), date!(2026 - 11 - 25));
        assert_eq!(before.as_of, None);
    }

    #[test]
    fn versus_compounds_the_difference() {
        let v = versus(dec!(0.12), dec!(0.10));
        assert_eq!(v.excess.round_dp(6), dec!(0.018182));
        assert_eq!(v.relative, Some(dec!(1.2)));
        let below = versus(dec!(0.05), dec!(0.10));
        assert!(below.excess < Decimal::ZERO);
        assert_eq!(versus(dec!(0.05), Decimal::ZERO).relative, None);
        assert_eq!(versus(dec!(0.05), dec!(-1)).excess, Decimal::ZERO);
    }

    #[test]
    fn monthly_table_chains_month_ends() {
        let series = points(&[
            (date!(2025 - 12 - 15), dec!(1.00)),
            (date!(2025 - 12 - 31), dec!(1.01)),
            (date!(2026 - 01 - 20), dec!(1.02)),
            (date!(2026 - 01 - 30), dec!(1.0302)),
            (date!(2026 - 03 - 02), dec!(1.0302)),
            (date!(2026 - 03 - 31), dec!(0.927180)),
        ]);
        let table = monthly_table(&series);
        assert_eq!(table.len(), 2);
        assert_eq!(table[0].year, 2025);
        assert_eq!(table[0].months[11], Some(dec!(0.01)));
        assert_eq!(table[0].from, date!(2025 - 12 - 15));
        let y2026 = &table[1];
        assert_eq!(y2026.months[0].map(|r| r.round_dp(6)), Some(dec!(0.02)));
        assert_eq!(y2026.months[1], None);
        assert_eq!(y2026.months[2].map(|r| r.round_dp(6)), Some(dec!(-0.1)));
        assert_eq!(y2026.from, date!(2025 - 12 - 31));
        assert_eq!(y2026.to, date!(2026 - 03 - 31));
        assert_eq!(
            y2026.total.round_dp(6),
            (dec!(0.927180) / dec!(1.01) - Decimal::ONE).round_dp(6)
        );
        // Um ponto só não faz mês.
        assert!(monthly_table(&series[..1]).is_empty());
        assert!(monthly_cells(&[]).is_empty());

        // As células levam a janela de cada mês e o acumulado desde o começo.
        let cells = monthly_cells(&series);
        assert_eq!(cells.len(), 3);
        assert_eq!(
            (cells[0].from, cells[0].to, cells[0].accumulated),
            (date!(2025 - 12 - 15), date!(2025 - 12 - 31), dec!(0.01))
        );
        assert_eq!(cells[1].from, date!(2025 - 12 - 31));
        assert_eq!(cells[1].accumulated, dec!(0.0302));
        assert_eq!(cells[2].month, Month::March);
        assert_eq!(cells[2].from, date!(2026 - 01 - 30));
        assert_eq!(cells[2].accumulated, dec!(0.927180) - Decimal::ONE);
    }

    #[test]
    fn percent_scale() {
        assert_eq!(to_percent(dec!(0.012345), 2), dec!(1.23));
        assert_eq!(
            market_value(&[units(Uuid::nil(), dec!(3), dec!(0.335))]),
            dec!(1.01)
        );
    }

    /// Um dia de mercado: o preço de cada ativo e quantas vezes a posição a
    /// pessoa comprou a mais no fim do dia (0 = não aportou).
    #[derive(Debug, Clone)]
    struct Day {
        prices: Vec<Decimal>,
        scale: u32,
    }

    fn arb_prices(assets: usize) -> impl Strategy<Value = Vec<Decimal>> {
        prop::collection::vec(100u32..100_000, assets).prop_map(|cents| {
            cents
                .into_iter()
                .map(|c| Decimal::new(i64::from(c), 2))
                .collect()
        })
    }

    fn arb_days(assets: usize) -> impl Strategy<Value = Vec<Day>> {
        let day = (arb_prices(assets), 0u32..3).prop_map(|(prices, scale)| Day { prices, scale });
        prop::collection::vec(day, 2..25)
    }

    /// Roda a série e devolve as cotas. Com `buy`, cada aporte compra mais da
    /// carteira na mesma proporção (multiplica as posições).
    fn run(ids: &[Uuid], initial: &[u32], days: &[Day], buy: bool) -> Vec<QuotaPoint> {
        let mut quantities: Vec<Decimal> = initial.iter().map(|q| Decimal::from(*q)).collect();
        let mut previous: Vec<Position> = Vec::new();
        let mut quota = None;
        let mut out = Vec::new();
        let start = date!(2026 - 01 - 01);
        for (n, day) in days.iter().enumerate() {
            let mut purchases = Vec::new();
            if buy && n > 0 && day.scale > 0 {
                for (i, q) in quantities.iter_mut().enumerate() {
                    let quantity = *q * Decimal::from(day.scale);
                    *q += quantity;
                    purchases.push(Purchase {
                        asset_id: ids[i],
                        quantity,
                        value: quantity * day.prices[i],
                    });
                }
            }
            let current: Vec<Position> = ids
                .iter()
                .zip(&quantities)
                .zip(&day.prices)
                .map(|((id, q), p)| units(*id, *q, *p))
                .collect();
            let mv: Decimal = current.iter().map(Position::value).sum();
            let flow = if quota.is_none() {
                mv
            } else {
                net_flow(&previous, &current, &purchases)
            };
            let next = advance(quota, mv, flow);
            out.push(QuotaPoint {
                date: start + Duration::days(n as i64),
                value: next.value,
            });
            quota = Some(next);
            previous = current;
        }
        out
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// Aporte comprado pelo preço do dia não muda a cota do dia, seja qual
        /// for o ativo e a quantidade.
        #[test]
        fn contribution_does_not_change_the_day(
            held in prop::collection::vec(0u32..500, 4),
            before in arb_prices(4),
            after in arb_prices(4),
            bought in prop::collection::vec(0u32..300, 4),
        ) {
            let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
            let position = |q: Decimal, i: usize, prices: &[Decimal]| units(ids[i], q, prices[i]);
            let previous: Vec<Position> =
                (0..4).map(|i| position(Decimal::from(held[i]), i, &before)).collect();
            let mv0 = market_value(&previous);
            let start = advance(None, mv0, mv0);
            let alone: Vec<Position> =
                (0..4).map(|i| position(Decimal::from(held[i]), i, &after)).collect();
            let with: Vec<Position> = (0..4)
                .map(|i| position(Decimal::from(held[i] + bought[i]), i, &after))
                .collect();
            let purchases: Vec<Purchase> = (0..4)
                .map(|i| Purchase {
                    asset_id: ids[i],
                    quantity: Decimal::from(bought[i]),
                    value: Decimal::from(bought[i]) * after[i],
                })
                .collect();
            let a = advance(Some(start), market_value(&alone), net_flow(&previous, &alone, &[]));
            let b = advance(Some(start), market_value(&with), net_flow(&previous, &with, &purchases));
            prop_assert_eq!(a.value, b.value);
        }

        /// Pagar acima do fechamento custa à cota, no máximo, a fração do
        /// custo sobre o que se pagou — seja qual for o tamanho da base. Antes,
        /// com base pequena, o mesmo custo podia levar a cota quase a zero.
        #[test]
        fn purchase_cost_never_costs_more_than_its_share_of_the_purchase(
            held in 1u32..2_000,
            bought in 1u32..5_000,
            before in 1_000u32..20_000,
            after in 1_000u32..20_000,
            premium in 0u32..1_000,
        ) {
            let id = Uuid::new_v4();
            let (before, after) = (Decimal::new(i64::from(before), 2), Decimal::new(i64::from(after), 2));
            let previous = [units(id, Decimal::from(held), before)];
            let start = advance(None, market_value(&previous), market_value(&previous));
            let current = [units(id, Decimal::from(held + bought), after)];
            let value = market_value(&current);
            let at_close = Decimal::from(bought) * after;
            let paid = round_cents(at_close * (Decimal::ONE + Decimal::new(i64::from(premium), 4)));
            let purchase = |value: Decimal| [Purchase { asset_id: id, quantity: Decimal::from(bought), value }];
            let fair = advance_day(Some(start), value, net_flow(&previous, &current, &purchase(at_close)), Decimal::ZERO);
            let cost = purchase_cost(&current, &purchase(paid));
            let charged = advance_day(Some(start), value, net_flow(&previous, &current, &purchase(paid)), cost);
            let share = (paid - at_close) / paid;
            prop_assert!(charged.value <= fair.value + dec!(0.000000001));
            prop_assert!(
                charged.value >= fair.value * (Decimal::ONE - share) - dec!(0.000001),
                "{} < {} × (1 − {share})", charged.value, fair.value
            );
        }

        /// Aportes que mantêm a proporção da carteira deixam a série inteira
        /// igual à de quem não aportou.
        #[test]
        fn contributions_do_not_change_the_return(
            initial in prop::collection::vec(1u32..500, 3),
            days in arb_days(3),
        ) {
            let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
            let without = run(&ids, &initial, &days, false);
            let with = run(&ids, &initial, &days, true);
            for (a, b) in without.iter().zip(&with) {
                // Diferença só de arredondamento (cota com 12 casas).
                prop_assert!((a.value - b.value).abs() < dec!(0.000001), "{} != {}", a.value, b.value);
            }
        }

        /// A rentabilidade de uma janela é a composição das janelas que a
        /// formam.
        #[test]
        fn periods_compose(
            initial in prop::collection::vec(1u32..500, 2),
            days in arb_days(2),
            cut in 0usize..100,
        ) {
            let ids: Vec<Uuid> = (0..2).map(|_| Uuid::new_v4()).collect();
            let series = run(&ids, &initial, &days, true);
            let first = series[0].date;
            let last = series[series.len() - 1].date;
            let middle = series[cut % series.len()].date;
            let whole = period_return(&series, first, last).unwrap().ret;
            let left = period_return(&series, first, middle).unwrap().ret;
            let right = period_return(&series, middle, last).unwrap().ret;
            let composed = (Decimal::ONE + left) * (Decimal::ONE + right) - Decimal::ONE;
            prop_assert!((whole - composed).abs() < dec!(0.0000000001), "{whole} != {composed}");
            // E a tabela mês × ano soma o mesmo que a série inteira.
            let year: Decimal = monthly_table(&series)
                .iter()
                .fold(Decimal::ONE, |acc, row| acc * (Decimal::ONE + row.total));
            prop_assert!((year - Decimal::ONE - whole).abs() < dec!(0.0000000001));
        }

        /// Índice diário: acumular em dois pedaços é o mesmo que de uma vez.
        #[test]
        fn daily_rates_compose(
            rates in prop::collection::vec(0u32..100_000, 1..40),
            cut in 0usize..40,
        ) {
            let start = date!(2026 - 01 - 01);
            let series: Vec<(Date, Decimal)> = rates
                .iter()
                .enumerate()
                .map(|(i, r)| (start + Duration::days(i as i64), Decimal::new(i64::from(*r), 6)))
                .collect();
            let end = start + Duration::days(series.len() as i64);
            let middle = start + Duration::days((cut % series.len()) as i64);
            let whole = daily_rate_accrued(&series, start, end).ret;
            let left = daily_rate_accrued(&series, start, middle).ret;
            let right = daily_rate_accrued(&series, middle, end).ret;
            let composed = (Decimal::ONE + left) * (Decimal::ONE + right) - Decimal::ONE;
            prop_assert!((whole - composed).abs() < dec!(0.000000000001));
        }
    }
}
