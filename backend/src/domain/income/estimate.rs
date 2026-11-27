//! A estimativa de renda da carteira de hoje: o que cada cota pagou nos
//! últimos 12 meses, **líquido** do IR na fonte do JCP, vezes a quantidade de
//! hoje. Regra pura; quem lê o banco é `crate::income`. O porquê de cada
//! decisão está em `docs/proventos-automaticos.md` ("Estimativa da carteira de
//! hoje").
//!
//! - **Fonte.** Para ações e FIIs da B3, os anúncios da própria B3
//!   (`b3_cash_announcements`): o rótulo diz o que é JCP, então dá o líquido, e
//!   o valor é o oficial. O Yahoo fica de reserva (a B3 sem o ativo ou fora do
//!   ar) e é a fonte de EUA, BDR e ETF. O Yahoo não separa JCP de dividendo:
//!   o que vem dele é bruto e líquido ao mesmo tempo.
//! - **Base de cotas de hoje.** O valor da B3 é por cota *da época*. Cada
//!   bonificação, desdobramento ou grupamento com data com no mesmo dia ou
//!   depois divide o valor (o Yahoo já vem assim: trata bonificação como
//!   desdobramento).
//! - **Janela.** 365 dias por **data ex**, `(hoje − 365, hoje]`, nas duas
//!   fontes. A data ex da B3 é o pregão seguinte à data com
//!   ([`ex_date_after`]), a mesma do Yahoo: trocar de fonte não mexe na borda.
//! - **Habitual.** A média anual de até 5 anos ([`usual`]) mostra quando os 12
//!   meses passaram bem do que o ativo costuma pagar ([`ABOVE_USUAL_RATIO`]).
//!   É só uma marca: o número principal continua a soma de 12 meses.

use rust_decimal::Decimal;
use time::{Date, Duration, OffsetDateTime};

use super::{amounts, jcp_withholding_rate, monthly_estimate, IncomeKind, TRAILING_DAYS};
use crate::domain::asset_class::AssetClass;
use crate::domain::entitlement::quantity_on;
use crate::domain::income_auto::{
    cash_kind, ex_date_after, CashAnnouncement, StockAnnouncement, StockEventKind,
};
use crate::domain::ledger::{Entry, LedgerError};

/// Quantos anos, no máximo, entram na média do habitual.
pub const USUAL_YEARS: u32 = 5;

/// Com menos que isto de histórico inteiro, não há "habitual" para comparar:
/// a média de um ano só é a própria soma de 12 meses.
pub const MIN_USUAL_YEARS: u32 = 2;

/// Os 12 meses estão "acima do habitual" quando passam de 1,5 × a média anual
/// (de até 5 anos, os 12 meses inclusive). Como a média inclui o ano de agora,
/// com 5 anos isso é o mesmo que os 12 meses passarem de ~1,71 × a média dos 4
/// anos anteriores. Medido numa carteira real em 27/11/2026: pega as
/// antecipações de dezembro de 2025 (de 1,5 a pouco mais de 2 vezes a média)
/// e deixa de fora quem só cresceu devagar.
pub const ABOVE_USUAL_RATIO: Decimal = Decimal::from_parts(15, 0, 0, false, 1);

/// Depois de quanto tempo sem busca bem-sucedida a B3 conta como fora do ar
/// para a estimativa: a busca é diária (`income_auto::FETCH_TTL`), então três
/// dias são várias rodadas falhando seguidas.
pub const B3_FRESH_FOR: Duration = Duration::days(3);

/// De onde veio o valor por cota de um ativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateSource {
    /// Os anúncios da B3: oficial, com o JCP separado.
    B3,
    /// O histórico do Yahoo: bruto, com o JCP misturado ao dividendo.
    Yahoo,
}

/// Um valor bruto e o líquido do IR na fonte do JCP. Fora do JCP os dois são
/// iguais.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GrossNet {
    pub gross: Decimal,
    pub net: Decimal,
}

impl GrossNet {
    fn add(self, other: GrossNet) -> GrossNet {
        GrossNet {
            gross: self.gross + other.gross,
            net: self.net + other.net,
        }
    }

    fn round_dp(self, places: u32) -> GrossNet {
        GrossNet {
            gross: self.gross.round_dp(places),
            net: self.net.round_dp(places),
        }
    }
}

/// Um pagamento por cota **de hoje**, na data ex, em reais.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitPayment {
    pub ex_date: Date,
    pub gross: Decimal,
    pub net: Decimal,
}

/// Se a data ex cai na janela de 12 meses: `(hoje − 365 dias, hoje]`.
pub fn in_window(ex_date: Date, today: Date) -> bool {
    ex_date > today.saturating_sub(Duration::days(TRAILING_DAYS)) && ex_date <= today
}

/// A data com mais velha cujo pagamento ainda pode cair na janela (a data ex é
/// no máximo dez dias depois dela): o que o banco precisa ler da B3.
pub fn b3_window_start(today: Date) -> Date {
    today.saturating_sub(Duration::days(TRAILING_DAYS + 10))
}

/// Quantas cotas de hoje uma cota com data com `record_date` virou: o produto
/// das bonificações e desdobramentos (`1 + fator ÷ 100`) e dos grupamentos
/// (`fator`) com data com **no mesmo dia ou depois** e data ex até hoje. No
/// mesmo dia conta porque quem tinha a cota na data com recebe o dinheiro só
/// pelas cotas velhas (a SBSP3 deu JCP e bonificação na mesma data com).
pub fn shares_after(record_date: Date, stock: &[StockAnnouncement], today: Date) -> Decimal {
    stock
        .iter()
        .filter(|s| s.record_date >= record_date && ex_date_after(s.record_date) <= today)
        .filter_map(|s| {
            let multiplier = match StockEventKind::of_label(&s.label)? {
                StockEventKind::Bonus | StockEventKind::Split => {
                    Decimal::ONE + s.factor / Decimal::ONE_HUNDRED
                }
                StockEventKind::ReverseSplit => s.factor,
            };
            (multiplier > Decimal::ZERO).then_some(multiplier)
        })
        .fold(Decimal::ONE, |acc, m| acc.checked_mul(m).unwrap_or(acc))
}

/// A alíquota do IR na fonte de um provento por cota: a do JCP pela data do
/// pagamento; sem data ("a definir"), a de hoje (o pagamento ainda vai cair).
fn withholding_rate(kind: IncomeKind, pay_date: Option<Date>, today: Date) -> Decimal {
    match kind {
        IncomeKind::Jcp => jcp_withholding_rate(pay_date.unwrap_or(today)),
        _ => Decimal::ZERO,
    }
}

/// Os anúncios da B3 de **um** ISIN como pagamentos por cota de hoje. Cada
/// parcela é um anúncio e conta pelo próprio valor (a B3 publica uma linha por
/// parcela, com a mesma data com).
pub fn b3_unit_payments(
    cash: &[CashAnnouncement],
    stock: &[StockAnnouncement],
    class: AssetClass,
    today: Date,
) -> Vec<UnitPayment> {
    cash.iter()
        .filter_map(|c| {
            let factor = shares_after(c.record_date, stock, today);
            let gross = c.rate.checked_div(factor)?;
            let kind = cash_kind(&c.label, class);
            let net = gross * (Decimal::ONE - withholding_rate(kind, c.pay_date, today));
            Some(UnitPayment {
                ex_date: ex_date_after(c.record_date),
                gross,
                net,
            })
        })
        .collect()
}

/// O histórico do Yahoo como pagamentos por cota de hoje, em reais (`rate` é o
/// câmbio da moeda do histórico). Sem JCP separado: líquido = bruto.
pub fn yahoo_unit_payments(points: &[super::DividendPoint], rate: Decimal) -> Vec<UnitPayment> {
    points
        .iter()
        .map(|p| {
            let value = p.amount * rate;
            UnitPayment {
                ex_date: p.ex_date,
                gross: value,
                net: value,
            }
        })
        .collect()
}

/// O que a cota pagou nos 12 meses, bruto e líquido.
pub fn trailing(payments: &[UnitPayment], today: Date) -> GrossNet {
    payments
        .iter()
        .filter(|p| in_window(p.ex_date, today))
        .fold(GrossNet::default(), |acc, p| {
            acc.add(GrossNet {
                gross: p.gross,
                net: p.net,
            })
        })
}

/// A média anual por cota do que o ativo costuma pagar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usual {
    /// Quantos anos de 365 dias entraram (de [`MIN_USUAL_YEARS`] a
    /// [`USUAL_YEARS`]).
    pub years: u32,
    /// Bruto por cota de hoje, por ano.
    pub per_unit_year: Decimal,
}

/// A média anual de até [`USUAL_YEARS`] anos: o ano de agora é `recent_gross`
/// (os 12 meses da fonte da estimativa, para a média e o número principal
/// contarem o mesmo ano), e os anteriores vêm do histórico longo
/// (`history`, o Yahoo: a B3 só cobre uns 12 meses).
///
/// Os anos são janelas de 365 dias, `(hoje − 365 × (k + 1), hoje − 365 × k]`.
/// A janela velha só entra **inteira**: quando o histórico tem pagamento de
/// antes do começo dela (a regra do Bazin, `analysis::references::bazin`). Uma
/// janela em que o ativo ainda não pagava, ou que o histórico não alcança, não
/// vira "zero". `None` sem histórico longo ou com menos de
/// [`MIN_USUAL_YEARS`] anos.
pub fn usual(history: &[UnitPayment], recent_gross: Decimal, today: Date) -> Option<Usual> {
    let first = history.iter().map(|p| p.ex_date).min()?;
    let mut total = recent_gross;
    let mut years = 1u32;
    for back in 1..USUAL_YEARS {
        let end = today.saturating_sub(Duration::days(TRAILING_DAYS * i64::from(back)));
        let start = today.saturating_sub(Duration::days(TRAILING_DAYS * i64::from(back + 1)));
        if first > start {
            break;
        }
        total += history
            .iter()
            .filter(|p| p.ex_date > start && p.ex_date <= end)
            .map(|p| p.gross)
            .sum::<Decimal>();
        years += 1;
    }
    (years >= MIN_USUAL_YEARS).then(|| Usual {
        years,
        per_unit_year: (total / Decimal::from(years)).round_dp(8),
    })
}

/// Se os 12 meses passaram bem do habitual: `12 meses > 1,5 × média anual`,
/// no bruto (o IR não muda o que é "acima"). Sem média, nunca.
pub fn above_usual(recent_gross: Decimal, usual: Option<&Usual>) -> bool {
    usual.is_some_and(|u| {
        u.per_unit_year > Decimal::ZERO && recent_gross > u.per_unit_year * ABOVE_USUAL_RATIO
    })
}

/// A média do habitual de um ativo, como a tela mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsualEstimate {
    pub years: u32,
    /// Por cota, por ano. O líquido usa a parte de JCP dos 12 meses (o
    /// histórico longo é do Yahoo, que não separa JCP): é aproximado.
    pub per_unit_year: GrossNet,
    /// Com a quantidade de hoje, por mês, em centavos.
    pub monthly: GrossNet,
}

/// A estimativa de um ativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetEstimate {
    pub source: EstimateSource,
    /// O que uma cota pagou nos 12 meses.
    pub per_unit_12m: GrossNet,
    /// `per_unit_12m × quantidade ÷ 12`, em centavos.
    pub monthly: GrossNet,
    pub usual: Option<UsualEstimate>,
    pub above_usual: bool,
}

/// A estimativa de um ativo: os 12 meses da fonte (`payments`), a média do
/// habitual pelo histórico longo (`history`, vazio quando não há) e a
/// quantidade de hoje.
pub fn asset_estimate(
    source: EstimateSource,
    payments: &[UnitPayment],
    history: &[UnitPayment],
    quantity: Decimal,
    today: Date,
) -> AssetEstimate {
    let per_unit_12m = trailing(payments, today).round_dp(8);
    let monthly = GrossNet {
        gross: monthly_estimate(per_unit_12m.gross, quantity),
        net: monthly_estimate(per_unit_12m.net, quantity),
    };
    let found = usual(history, per_unit_12m.gross, today);
    let usual_estimate = found.map(|u| {
        let net_share = if per_unit_12m.gross > Decimal::ZERO {
            per_unit_12m.net / per_unit_12m.gross
        } else {
            Decimal::ONE
        };
        let per_unit_year = GrossNet {
            gross: u.per_unit_year,
            net: (u.per_unit_year * net_share).round_dp(8),
        };
        UsualEstimate {
            years: u.years,
            per_unit_year,
            monthly: GrossNet {
                gross: monthly_estimate(per_unit_year.gross, quantity),
                net: monthly_estimate(per_unit_year.net, quantity),
            },
        }
    });
    AssetEstimate {
        source,
        per_unit_12m,
        monthly,
        usual: usual_estimate,
        above_usual: above_usual(per_unit_12m.gross, found.as_ref()),
    }
}

/// Qual fonte vale para um ativo da B3. `b3_fetched_at` é a última busca
/// bem-sucedida do emissor, e só existe quando a B3 tem o ativo (o ISIN dele
/// foi achado); `yahoo` diz se há histórico do Yahoo em cache.
///
/// - B3 buscada há até [`B3_FRESH_FOR`]: B3.
/// - B3 velha (fora do ar há dias) e Yahoo em cache: Yahoo.
/// - B3 velha e sem Yahoo: a B3 velha, que é melhor que nada.
/// - Sem B3: Yahoo, se houver; senão nenhuma (o ativo vai para `missing`).
pub fn pick_source(
    b3_fetched_at: Option<OffsetDateTime>,
    yahoo: bool,
    now: OffsetDateTime,
) -> Option<EstimateSource> {
    match b3_fetched_at {
        Some(at) if now - at <= B3_FRESH_FOR || !yahoo => Some(EstimateSource::B3),
        _ if yahoo => Some(EstimateSource::Yahoo),
        _ => None,
    }
}

/// O que a posição teria rendido nos 12 meses pela B3, com as cotas que o livro
/// tinha na data com de cada anúncio: o valor da B3 é por cota da época, então
/// não há base a converter. Cada pagamento é arredondado como o provento
/// automático ([`amounts`]); o JCP perde o IR pela data de pagamento.
pub fn b3_trailing_income(
    cash: &[CashAnnouncement],
    class: AssetClass,
    entries: &[Entry],
    today: Date,
) -> Result<GrossNet, LedgerError> {
    let mut total = GrossNet::default();
    for c in cash
        .iter()
        .filter(|c| in_window(ex_date_after(c.record_date), today))
    {
        let quantity = quantity_on(entries, c.record_date)?;
        if quantity <= Decimal::ZERO {
            continue;
        }
        let kind = cash_kind(&c.label, class);
        let pay_date = c.pay_date.unwrap_or(today);
        if let Ok(values) = amounts(kind, pay_date, c.rate, quantity, None) {
            total = total.add(GrossNet {
                gross: values.gross,
                net: values.net,
            });
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests;
