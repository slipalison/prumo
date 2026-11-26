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
        vacancy_reference_date: vacancy.map(|v| v.reference_date),
        synced_at,
    }
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
