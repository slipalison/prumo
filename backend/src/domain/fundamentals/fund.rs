//! Os indicadores de um FII: o informe mensal da CVM dá o patrimônio líquido e
//! as cotas emitidas (valor patrimonial por cota, e com a cotação do Prumo o
//! P/VP); o informe trimestral dá, imóvel a imóvel, a vacância declarada.
//!
//! **Vacância física.** O informe trimestral (`inf_trimestral_fii_imovel`)
//! traz, para cada imóvel, a área e o percentual vago (0 a 1). O Prumo faz a
//! média ponderada pela área dos *imóveis para renda acabados* do informe mais
//! novo de cada fundo. É a vacância física, não a financeira (a CVM não publica
//! a receita perdida por imóvel), e só vale para fundo de tijolo: fundo de
//! papel ou de fundos não tem imóvel para renda ([`Gap::NoRentalProperty`]).
//!
//! **Caixa.** O informe mensal traz o ativo total (`Valor_Ativo`, no
//! `complemento`) e o grupo "ativos para manter liquidez"
//! (`Total_Necessidades_Liquidez`, no `ativo_passivo`): disponibilidades,
//! títulos públicos, títulos privados e fundos de renda fixa. O % de caixa é um
//! sobre o outro ([`cash_share`]). Conferido no informe de 2026 (13 mil linhas):
//! o grupo é a soma das quatro contas e o ativo total é a soma dos três grupos
//! do balanço (liquidez, investido e a receber).

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};

use super::{Figure, Gap, RATIO_DP};

/// O informe mensal mais novo de um fundo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundReport {
    pub cnpj: String,
    /// As quatro letras do emissor, da raiz do ISIN da cota (`KNRI`).
    pub issuer: String,
    pub isin: String,
    pub name: String,
    pub reference_date: Date,
    pub version: i32,
    pub net_assets: Option<Decimal>,
    pub shares: Option<Decimal>,
    /// `Valor_Ativo`: o ativo total.
    pub total_assets: Option<Decimal>,
    /// `Total_Necessidades_Liquidez`: o que o fundo tem em caixa e aplicações
    /// de liquidez.
    pub liquid_assets: Option<Decimal>,
}

/// O que o fundo tem em caixa e aplicações de liquidez sobre o ativo total, de
/// 0 a 1. Ativo total zero ou negativo: [`Gap::NonPositiveBase`]; liquidez
/// negativa ou maior que o ativo total (há três casos assim no informe de
/// 2026): erro de preenchimento, [`Gap::Implausible`].
pub fn cash_share(liquid_assets: Option<Decimal>, total_assets: Option<Decimal>) -> Figure {
    let share = liquid_assets
        .map_or(Figure::missing(Gap::NotReported), Figure::known)
        .ratio(total_assets.map_or(Figure::missing(Gap::NotReported), Figure::known));
    match share.value() {
        Some(v) if v < Decimal::ZERO || v > Decimal::ONE => Figure::missing(Gap::Implausible),
        _ => share,
    }
}

/// Um imóvel para renda acabado do informe trimestral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RentalProperty {
    /// Área em m², como o fundo declara.
    pub area: Option<Decimal>,
    /// Fração vaga, de 0 a 1.
    pub vacancy: Option<Decimal>,
}

/// A vacância de um fundo num informe trimestral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vacancy {
    pub reference_date: Date,
    pub value: Figure,
    /// Imóveis que entraram na média.
    pub properties: u32,
}

/// A média da vacância ponderada pela área. Imóvel sem área positiva ou com
/// vacância fora de 0 a 1 (erro de preenchimento) fica fora da média.
pub fn physical_vacancy(properties: &[RentalProperty]) -> (Figure, u32) {
    let valid: Vec<(Decimal, Decimal)> = properties
        .iter()
        .filter_map(|p| {
            let area = p.area.filter(|a| *a > Decimal::ZERO)?;
            let vacancy = p
                .vacancy
                .filter(|v| (Decimal::ZERO..=Decimal::ONE).contains(v))?;
            Some((area, vacancy))
        })
        .collect();
    let total: Decimal = valid.iter().map(|(area, _)| *area).sum();
    if valid.is_empty() || total <= Decimal::ZERO {
        return (Figure::missing(Gap::NoRentalProperty), 0);
    }
    let vacant: Decimal = valid.iter().map(|(area, vacancy)| area * vacancy).sum();
    let count = u32::try_from(valid.len()).unwrap_or(u32::MAX);
    match vacant.checked_div(total) {
        Some(v) => (Figure::known(v.round_dp(RATIO_DP).normalize()), count),
        None => (Figure::missing(Gap::NoRentalProperty), 0),
    }
}

/// Os indicadores de um FII. Contrato estável: ver `docs/fundamentos.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundMetrics {
    pub cnpj: String,
    pub issuer: String,
    pub name: String,
    /// A data do informe mensal usado.
    pub reference_date: Date,
    pub net_assets: Figure,
    pub shares: Figure,
    /// Patrimônio líquido ÷ cotas emitidas.
    pub nav_per_share: Figure,
    /// A cotação do Prumo usada no P/VP.
    pub price: Option<Decimal>,
    /// Cotação ÷ valor patrimonial por cota.
    pub price_to_book: Figure,
    /// Vacância física (0 a 1) do informe trimestral mais novo.
    pub vacancy: Figure,
    /// O ativo total do informe mensal.
    pub total_assets: Figure,
    /// Caixa e aplicações de liquidez ÷ ativo total, de 0 a 1 ([`cash_share`]).
    pub cash_share: Figure,
    pub vacancy_reference_date: Option<Date>,
    pub synced_at: Option<OffsetDateTime>,
}

pub fn fund_metrics(
    report: &FundReport,
    vacancy: Option<&Vacancy>,
    price: Option<Decimal>,
    synced_at: Option<OffsetDateTime>,
) -> FundMetrics {
    let net_assets = report
        .net_assets
        .map_or(Figure::missing(Gap::NotReported), Figure::known);
    let shares = report
        .shares
        .filter(|s| *s > Decimal::ZERO)
        .map_or(Figure::missing(Gap::NoShares), Figure::known);
    let nav_per_share = net_assets.ratio(shares);
    let price_to_book = price
        .map_or(Figure::missing(Gap::NoQuote), Figure::known)
        .ratio(nav_per_share);
    FundMetrics {
        cnpj: report.cnpj.clone(),
        issuer: report.issuer.clone(),
        name: report.name.clone(),
        reference_date: report.reference_date,
        net_assets,
        shares,
        nav_per_share,
        price,
        price_to_book,
        vacancy: vacancy.map_or(Figure::missing(Gap::NoQuarterlyReport), |v| v.value),
        total_assets: report
            .total_assets
            .map_or(Figure::missing(Gap::NotReported), Figure::known),
        cash_share: cash_share(report.liquid_assets, report.total_assets),
        vacancy_reference_date: vacancy.map(|v| v.reference_date),
        synced_at,
    }
}

/// O teto do rendimento de um mês que o Prumo aceita como número (10%: 120%
/// ao ano). Acima disso, ou negativo, é erro de preenchimento: há fundo que
/// escreve 0,89 querendo dizer 0,89% (medido no informe de 2026).
pub const MAX_MONTHLY_YIELD: Decimal = Decimal::from_parts(1, 0, 0, false, 1);

/// Um mês do informe mensal de um FII, como o fundo informou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundMonth {
    pub cnpj: String,
    /// `Data_Referencia`: o primeiro dia do mês do informe.
    pub reference_date: Date,
    pub version: i32,
    pub net_assets: Option<Decimal>,
    pub shares: Option<Decimal>,
    /// `Percentual_Dividend_Yield_Mes`, em fração: o rendimento do mês sobre o
    /// valor patrimonial da cota (não sobre a cotação).
    pub dividend_yield: Option<Decimal>,
}

/// Um mês da série do fundo, pronto para a tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundMonthFigures {
    pub reference_date: Date,
    /// Patrimônio líquido ÷ cotas emitidas no fim do mês.
    pub nav_per_share: Figure,
    /// O rendimento do mês sobre o valor patrimonial, como o fundo informou;
    /// fora de 0 a [`MAX_MONTHLY_YIELD`], [`Gap::Implausible`].
    pub dividend_yield: Figure,
}

/// Os `count` meses mais novos do fundo, do mais velho ao mais novo. Um mês
/// repetido (reapresentação) vale pela versão maior.
pub fn fund_months(months: &[FundMonth], count: usize) -> Vec<FundMonthFigures> {
    let mut newest: std::collections::BTreeMap<Date, &FundMonth> =
        std::collections::BTreeMap::new();
    for month in months {
        if newest
            .get(&month.reference_date)
            .is_some_and(|kept| kept.version >= month.version)
        {
            continue;
        }
        newest.insert(month.reference_date, month);
    }
    let skip = newest.len().saturating_sub(count);
    newest
        .into_values()
        .skip(skip)
        .map(|m| {
            let net_assets = m
                .net_assets
                .map_or(Figure::missing(Gap::NotReported), Figure::known);
            let shares = m
                .shares
                .filter(|s| *s > Decimal::ZERO)
                .map_or(Figure::missing(Gap::NoShares), Figure::known);
            let dividend_yield = match m.dividend_yield {
                None => Figure::missing(Gap::NotReported),
                Some(v) if v < Decimal::ZERO || v > MAX_MONTHLY_YIELD => {
                    Figure::missing(Gap::Implausible)
                }
                Some(v) => Figure::known(v.round_dp(RATIO_DP).normalize()),
            };
            FundMonthFigures {
                reference_date: m.reference_date,
                nav_per_share: net_assets.ratio(shares),
                dividend_yield,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn report(net_assets: Option<Decimal>, shares: Option<Decimal>) -> FundReport {
        FundReport {
            cnpj: "00.000.000/0001-91".into(),
            issuer: "FICT".into(),
            isin: "BRFICTCTF000".into(),
            name: "FUNDO FICTICIO FII".into(),
            reference_date: date!(2026 - 10 - 31),
            version: 1,
            net_assets,
            shares,
            total_assets: Some(dec!(1200000)),
            liquid_assets: Some(dec!(60000)),
        }
    }

    fn property(area: &str, vacancy: &str) -> RentalProperty {
        RentalProperty {
            area: area.parse().ok(),
            vacancy: vacancy.parse().ok(),
        }
    }

    #[test]
    fn book_value_per_share_and_price_to_book() {
        let m = fund_metrics(
            &report(Some(dec!(1000000)), Some(dec!(10000))),
            None,
            Some(dec!(90)),
            None,
        );
        assert_eq!(m.nav_per_share.value(), Some(dec!(100)));
        assert_eq!(m.price_to_book.value(), Some(dec!(0.9)));
        assert_eq!(m.vacancy.gap(), Some(Gap::NoQuarterlyReport));
        assert_eq!(m.vacancy_reference_date, None);
        assert_eq!(m.total_assets.value(), Some(dec!(1200000)));
        assert_eq!(m.cash_share.value(), Some(dec!(0.05)));
    }

    #[test]
    fn cash_is_the_liquid_group_over_total_assets() {
        assert_eq!(
            cash_share(Some(dec!(25)), Some(dec!(100))).value(),
            Some(dec!(0.25))
        );
        assert_eq!(
            cash_share(Some(dec!(0)), Some(dec!(100))).value(),
            Some(dec!(0))
        );
        assert_eq!(
            cash_share(Some(dec!(100)), Some(dec!(100))).value(),
            Some(dec!(1))
        );
        // Sem um dos dois números, o motivo; ativo zero, base não positiva.
        assert_eq!(
            cash_share(None, Some(dec!(100))).gap(),
            Some(Gap::NotReported)
        );
        assert_eq!(
            cash_share(Some(dec!(1)), None).gap(),
            Some(Gap::NotReported)
        );
        assert_eq!(
            cash_share(Some(dec!(1)), Some(dec!(0))).gap(),
            Some(Gap::NonPositiveBase)
        );
        // Mais caixa que ativo, ou caixa negativo: erro de preenchimento.
        assert_eq!(
            cash_share(Some(dec!(150)), Some(dec!(100))).gap(),
            Some(Gap::Implausible)
        );
        assert_eq!(
            cash_share(Some(dec!(-1)), Some(dec!(100))).gap(),
            Some(Gap::Implausible)
        );
    }

    #[test]
    fn missing_pieces_have_reasons() {
        let no_price = fund_metrics(&report(Some(dec!(1000)), Some(dec!(10))), None, None, None);
        assert_eq!(no_price.price_to_book.gap(), Some(Gap::NoQuote));
        let no_shares = fund_metrics(
            &report(Some(dec!(1000)), Some(dec!(0))),
            None,
            Some(dec!(9)),
            None,
        );
        assert_eq!(no_shares.nav_per_share.gap(), Some(Gap::NoShares));
        assert_eq!(no_shares.price_to_book.gap(), Some(Gap::NoShares));
        let no_equity = fund_metrics(&report(None, Some(dec!(10))), None, Some(dec!(9)), None);
        assert_eq!(no_equity.nav_per_share.gap(), Some(Gap::NotReported));
        let negative = fund_metrics(
            &report(Some(dec!(-10)), Some(dec!(10))),
            None,
            Some(dec!(9)),
            None,
        );
        assert_eq!(negative.nav_per_share.value(), Some(dec!(-1)));
        assert_eq!(negative.price_to_book.gap(), Some(Gap::NonPositiveBase));
    }

    #[test]
    fn vacancy_is_weighted_by_area() {
        let (v, n) = physical_vacancy(&[property("1000", "0"), property("3000", "0.2")]);
        assert_eq!(v.value(), Some(dec!(0.15)));
        assert_eq!(n, 2);
        // Fora de 0 a 1, área zero ou vazia: fora da média.
        let (v, n) = physical_vacancy(&[
            property("1000", "0.1"),
            property("500", "-0.3"),
            property("500", "12"),
            property("0", "0.9"),
            property("", "0.9"),
            property("800", ""),
        ]);
        assert_eq!(v.value(), Some(dec!(0.1)));
        assert_eq!(n, 1);
        let (v, n) = physical_vacancy(&[]);
        assert_eq!(v.gap(), Some(Gap::NoRentalProperty));
        assert_eq!(n, 0);
        let vacancy = Vacancy {
            reference_date: date!(2026 - 09 - 30),
            value: Figure::known(dec!(0.05)),
            properties: 3,
        };
        let m = fund_metrics(
            &report(Some(dec!(10)), Some(dec!(1))),
            Some(&vacancy),
            None,
            None,
        );
        assert_eq!(m.vacancy.value(), Some(dec!(0.05)));
        assert_eq!(m.vacancy_reference_date, Some(date!(2026 - 09 - 30)));
    }

    fn month(day: Date, version: i32, assets: &str, shares: &str, dy: &str) -> FundMonth {
        FundMonth {
            cnpj: "00.000.000/0001-91".into(),
            reference_date: day,
            version,
            net_assets: assets.parse().ok(),
            shares: shares.parse().ok(),
            dividend_yield: dy.parse().ok(),
        }
    }

    #[test]
    fn the_monthly_series_keeps_the_newest_months_and_the_highest_version() {
        let months = vec![
            month(date!(2026 - 08 - 01), 1, "1000000", "10000", "0.0067"),
            month(date!(2026 - 10 - 01), 1, "990000", "10000", "0.0070"),
            month(date!(2026 - 09 - 01), 1, "1005000", "10000", "0.0068"),
            // A reapresentação de setembro vence.
            month(date!(2026 - 09 - 01), 2, "1010000", "10000", "0.0069"),
            month(date!(2026 - 07 - 01), 1, "1000000", "10000", "0.0066"),
        ];
        let series = fund_months(&months, 3);
        assert_eq!(
            series.iter().map(|m| m.reference_date).collect::<Vec<_>>(),
            vec![
                date!(2026 - 08 - 01),
                date!(2026 - 09 - 01),
                date!(2026 - 10 - 01)
            ]
        );
        assert_eq!(series[1].nav_per_share.value(), Some(dec!(101)));
        assert_eq!(series[1].dividend_yield.value(), Some(dec!(0.0069)));
        assert_eq!(series[2].nav_per_share.value(), Some(dec!(99)));
    }

    #[test]
    fn an_impossible_monthly_yield_is_a_filling_error() {
        let months = vec![
            month(date!(2026 - 08 - 01), 1, "1000", "10", "0.89"),
            month(date!(2026 - 09 - 01), 1, "1000", "10", "-0.002"),
            month(date!(2026 - 10 - 01), 1, "1000", "0", ""),
            month(date!(2026 - 11 - 01), 1, "", "10", "0.1"),
        ];
        let series = fund_months(&months, 12);
        assert_eq!(series[0].dividend_yield.gap(), Some(Gap::Implausible));
        assert_eq!(series[1].dividend_yield.gap(), Some(Gap::Implausible));
        assert_eq!(series[2].dividend_yield.gap(), Some(Gap::NotReported));
        assert_eq!(series[2].nav_per_share.gap(), Some(Gap::NoShares));
        // 10% ainda é número; sem patrimônio, o motivo.
        assert_eq!(series[3].dividend_yield.value(), Some(dec!(0.1)));
        assert_eq!(series[3].nav_per_share.gap(), Some(Gap::NotReported));
        assert!(fund_months(&[], 12).is_empty());
    }

    proptest! {
        #[test]
        fn vacancy_stays_between_zero_and_one(rows in proptest::collection::vec((0i64..100_000, 0i64..=100), 1..30)) {
            let properties: Vec<RentalProperty> = rows
                .iter()
                .map(|(area, pct)| RentalProperty { area: Some(Decimal::from(*area)), vacancy: Some(Decimal::from(*pct) / dec!(100)) })
                .collect();
            let (v, _) = physical_vacancy(&properties);
            if let Some(v) = v.value() {
                prop_assert!(v >= Decimal::ZERO && v <= Decimal::ONE);
            } else {
                prop_assert!(rows.iter().all(|(area, _)| *area == 0));
            }
        }
    }
}
