//! Setor e segmento de um papel: de onde vêm e como achar a linha certa da
//! tabela de referência a partir do símbolo.
//!
//! Só duas classes têm fonte pública e gratuita que o Prumo usa: Ações
//! Nacionais (Classificação Setorial da B3) e Fundos Imobiliários (informe
//! mensal da CVM). O resto fica "sem setor" — e isso é dito, não escondido.

use rust_decimal::Decimal;
use serde::Serialize;

use super::asset_class::AssetClass;

/// De onde veio a classificação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectorSource {
    /// Planilha de Classificação Setorial da B3 (ações).
    B3Stocks,
    /// Informe mensal de FII da CVM (dados abertos).
    CvmReits,
}

impl SectorSource {
    pub const ALL: [SectorSource; 2] = [SectorSource::B3Stocks, SectorSource::CvmReits];

    pub fn as_str(self) -> &'static str {
        match self {
            SectorSource::B3Stocks => "b3_stocks",
            SectorSource::CvmReits => "cvm_reits",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        SectorSource::ALL.into_iter().find(|s| s.as_str() == value)
    }

    /// A classe que a fonte classifica.
    pub fn class(self) -> AssetClass {
        match self {
            SectorSource::B3Stocks => AssetClass::BrStocks,
            SectorSource::CvmReits => AssetClass::BrReits,
        }
    }
}

/// O que a referência diz de um papel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorInfo {
    pub sector: String,
    pub subsector: Option<String>,
    pub segment: Option<String>,
}

/// O tipo de um FII, que o Prumo deriva do balanço do informe mensal da CVM
/// (`sectors::cvm`) e grava como `sector` do fundo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FundType {
    Brick,
    Paper,
    Hybrid,
    FundOfFunds,
}

impl FundType {
    pub const ALL: [FundType; 4] = [
        FundType::Brick,
        FundType::Paper,
        FundType::Hybrid,
        FundType::FundOfFunds,
    ];

    /// O texto gravado em `sector_reference.sector` (e que a pessoa lê).
    pub const fn label(self) -> &'static str {
        match self {
            FundType::Brick => "Tijolo",
            FundType::Paper => "Papel",
            FundType::Hybrid => "Híbrido",
            FundType::FundOfFunds => "Fundo de fundos",
        }
    }

    /// O tipo pelo texto do setor; `None` para o que não é tipo de FII.
    pub fn from_label(label: &str) -> Option<Self> {
        FundType::ALL.into_iter().find(|t| t.label() == label)
    }
}

/// O que a carteira de um FII tem, pelo balanço do último informe mensal: a
/// fração em imóveis (`brick`), em recebíveis (`paper`: CRI, LCI, LIG, letras
/// hipotecárias) e em cotas de outros FIIs (`funds`). As três somam 1; caixa e
/// títulos públicos ficam fora da conta, como no tipo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundComposition {
    pub brick: Decimal,
    pub paper: Decimal,
    pub funds: Decimal,
}

/// Casas das frações da composição.
pub const COMPOSITION_DP: u32 = 6;

impl FundComposition {
    /// A composição pelos valores do balanço. `None` quando a soma não é
    /// positiva (fundo sem carteira ainda) ou algum valor é negativo.
    pub fn from_amounts(brick: Decimal, paper: Decimal, funds: Decimal) -> Option<Self> {
        if brick < Decimal::ZERO || paper < Decimal::ZERO || funds < Decimal::ZERO {
            return None;
        }
        let base = brick + paper + funds;
        if base <= Decimal::ZERO {
            return None;
        }
        let brick = (brick / base).round_dp(COMPOSITION_DP);
        let paper = (paper / base)
            .round_dp(COMPOSITION_DP)
            .min(Decimal::ONE - brick);
        Some(Self {
            brick,
            paper,
            // O resto, para as três somarem 1 exatamente.
            funds: Decimal::ONE - brick - paper,
        })
    }

    /// A composição como o banco a guarda; `None` se ela não soma 1 ou tem
    /// fração fora de 0 a 1.
    pub fn from_shares(brick: Decimal, paper: Decimal, funds: Decimal) -> Option<Self> {
        let unit = |v: Decimal| (Decimal::ZERO..=Decimal::ONE).contains(&v);
        (unit(brick) && unit(paper) && unit(funds) && brick + paper + funds == Decimal::ONE).then(
            || Self {
                brick: brick.normalize(),
                paper: paper.normalize(),
                funds: funds.normalize(),
            },
        )
    }
}

/// A fonte e o código de quatro caracteres do emissor a consultar para um
/// ativo. `WEGE3` → (B3, `WEGE`); `KNRI11` → (CVM, `KNRI`); `PETR4F` (mercado
/// fracionário) → (B3, `PETR`). `None` para o que não tem fonte (classe sem
/// ela ou símbolo que não é de ticker da B3).
pub fn reference_key(class: AssetClass, symbol: &str) -> Option<(SectorSource, String)> {
    let source = SectorSource::ALL.into_iter().find(|s| s.class() == class)?;
    let upper = symbol.trim().to_uppercase();
    if !upper.is_ascii() || upper.len() < 5 {
        return None;
    }
    let (root, tail) = upper.split_at(4);
    let digits = tail.strip_suffix('F').unwrap_or(tail);
    let root_ok = root.chars().all(|c| c.is_ascii_alphanumeric())
        && root.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    let tail_ok = (1..=2).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit());
    (root_ok && tail_ok).then(|| (source, root.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_round_trip_and_know_their_class() {
        for source in SectorSource::ALL {
            assert_eq!(SectorSource::parse(source.as_str()), Some(source));
        }
        assert_eq!(SectorSource::parse("sec"), None);
        assert_eq!(SectorSource::B3Stocks.class(), AssetClass::BrStocks);
        assert_eq!(SectorSource::CvmReits.class(), AssetClass::BrReits);
    }

    #[test]
    fn the_key_is_the_issuer_code() {
        let key = |class, symbol| reference_key(class, symbol);
        assert_eq!(
            key(AssetClass::BrStocks, "WEGE3"),
            Some((SectorSource::B3Stocks, "WEGE".into()))
        );
        assert_eq!(
            key(AssetClass::BrStocks, "petr4f"),
            Some((SectorSource::B3Stocks, "PETR".into()))
        );
        // O código do emissor pode ter dígito: B3SA3, AZUL4.
        assert_eq!(
            key(AssetClass::BrStocks, "B3SA3"),
            Some((SectorSource::B3Stocks, "B3SA".into()))
        );
        assert_eq!(
            key(AssetClass::BrReits, "KNRI11"),
            Some((SectorSource::CvmReits, "KNRI".into()))
        );
    }

    #[test]
    fn fund_types_round_trip_through_the_label() {
        for kind in FundType::ALL {
            assert_eq!(FundType::from_label(kind.label()), Some(kind));
        }
        assert_eq!(FundType::from_label("Hibrido"), None);
        assert_eq!(FundType::from_label("Bancos"), None);
        assert_eq!(
            serde_json::to_value(FundType::FundOfFunds).unwrap(),
            "fund_of_funds"
        );
    }

    #[test]
    fn the_composition_is_the_share_of_each_part_and_sums_to_one() {
        use rust_decimal_macros::dec;
        let mix = FundComposition::from_amounts(dec!(500), dec!(300), dec!(200)).unwrap();
        assert_eq!(
            (mix.brick, mix.paper, mix.funds),
            (dec!(0.5), dec!(0.3), dec!(0.2))
        );
        let thirds = FundComposition::from_amounts(dec!(1), dec!(1), dec!(1)).unwrap();
        assert_eq!(thirds.brick + thirds.paper + thirds.funds, Decimal::ONE);
        assert_eq!(
            FundComposition::from_amounts(dec!(0), dec!(0), dec!(0)),
            None
        );
        assert_eq!(
            FundComposition::from_amounts(dec!(-1), dec!(2), dec!(0)),
            None
        );
        assert_eq!(
            FundComposition::from_shares(dec!(0.5), dec!(0.3), dec!(0.2)),
            Some(mix)
        );
        assert_eq!(
            FundComposition::from_shares(dec!(0.5), dec!(0.3), dec!(0.3)),
            None
        );
        assert_eq!(
            FundComposition::from_shares(dec!(1.2), dec!(-0.2), dec!(0)),
            None
        );
    }

    proptest::proptest! {
        #[test]
        fn any_balance_gives_shares_between_zero_and_one_that_sum_to_one(
            brick in 0i64..10_000_000, paper in 0i64..10_000_000, funds in 0i64..10_000_000,
        ) {
            let mix = FundComposition::from_amounts(Decimal::from(brick), Decimal::from(paper), Decimal::from(funds));
            if brick + paper + funds == 0 {
                proptest::prop_assert!(mix.is_none());
            } else {
                let mix = mix.unwrap();
                proptest::prop_assert_eq!(mix.brick + mix.paper + mix.funds, Decimal::ONE);
                for part in [mix.brick, mix.paper, mix.funds] {
                    proptest::prop_assert!(part >= Decimal::ZERO && part <= Decimal::ONE);
                }
                proptest::prop_assert_eq!(FundComposition::from_shares(mix.brick, mix.paper, mix.funds), Some(mix));
            }
        }
    }

    #[test]
    fn classes_without_a_source_and_odd_symbols_have_no_key() {
        assert_eq!(reference_key(AssetClass::IntlStocks, "AAPL"), None);
        assert_eq!(reference_key(AssetClass::UsReits, "O"), None);
        assert_eq!(reference_key(AssetClass::Crypto, "BTC"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "WEGE"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "WEGE333"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "WEGEX"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "1EGE3"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, "AÇÃO3"), None);
        assert_eq!(reference_key(AssetClass::BrStocks, ""), None);
    }
}
