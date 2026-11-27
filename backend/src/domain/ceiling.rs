//! O teto de preço de um ativo: **a escolha da pessoa** e o valor que vale hoje.
//!
//! O teto é opcional e é dela (`docs/alocacao.md`): o Prumo informa, nunca
//! impõe nem pré-preenche, e ativo novo nasce sem teto. A escolha é uma de:
//!
//! - **sem teto**;
//! - **um método** ([`CeilingMethod`]): grava-se o método, e o valor é
//!   recalculado a cada leitura com o balanço, os proventos e as taxas de hoje
//!   ([`effective`]). Ações aceitam Bazin, Graham e "Pelas duas"; FIIs, VP,
//!   Rendimento e "Pelas duas" ([`ReferenceFamily`]);
//! - **valor próprio**: um número fixo, em reais por unidade.
//!
//! Método sem valor hoje (prejuízo zera o Graham, FII sem proventos, sem VP)
//! **não cai calado para "sem teto"**: o teto fica [`Ceiling::Unavailable`], com
//! o motivo, e o motor tira o ativo do aporte dizendo por quê.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::analysis::fund_references::FundReferences;
use super::analysis::references::{PriceReferences, ReferenceGap};
use super::asset_class::{AssetClass, Market};

/// Um método de referência que a pessoa pode escolher como teto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CeilingMethod {
    /// Ações: a média de dividendos ÷ rendimento-alvo.
    Bazin,
    /// Ações: `√(22,5 × LPA × VPA)`.
    Graham,
    /// O menor das duas referências da família (Bazin e Graham; VP e
    /// Rendimento).
    Combined,
    /// FIIs: o valor patrimonial por cota.
    Nav,
    /// FIIs: os proventos de 12 meses ÷ o alvo do tipo.
    FundYield,
}

impl CeilingMethod {
    pub const ALL: [CeilingMethod; 5] = [
        CeilingMethod::Bazin,
        CeilingMethod::Graham,
        CeilingMethod::Combined,
        CeilingMethod::Nav,
        CeilingMethod::FundYield,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CeilingMethod::Bazin => "bazin",
            CeilingMethod::Graham => "graham",
            CeilingMethod::Combined => "combined",
            CeilingMethod::Nav => "nav",
            CeilingMethod::FundYield => "fund_yield",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        CeilingMethod::ALL.into_iter().find(|m| m.as_str() == value)
    }
}

/// Que referências um ativo tem: as de ação ou as de FII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceFamily {
    Stock,
    Fund,
}

impl ReferenceFamily {
    /// Ações Nacionais e FIIs da B3 têm referências; o resto (exterior,
    /// cripto, renda fixa, ETF e BDR noutras classes) não.
    pub fn of(class: AssetClass, market: Market) -> Option<Self> {
        match (class, market) {
            (AssetClass::BrStocks, Market::B3) => Some(ReferenceFamily::Stock),
            (AssetClass::BrReits, Market::B3) => Some(ReferenceFamily::Fund),
            _ => None,
        }
    }

    /// Os métodos que a família aceita, na ordem da tela.
    pub fn methods(self) -> [CeilingMethod; 3] {
        match self {
            ReferenceFamily::Stock => [
                CeilingMethod::Bazin,
                CeilingMethod::Graham,
                CeilingMethod::Combined,
            ],
            ReferenceFamily::Fund => [
                CeilingMethod::Nav,
                CeilingMethod::FundYield,
                CeilingMethod::Combined,
            ],
        }
    }

    pub fn accepts(self, method: CeilingMethod) -> bool {
        self.methods().contains(&method)
    }
}

/// Se o ativo de `class` em `market` aceita `method` como teto. A mensagem vai
/// sem acento (atravessa log).
pub fn check_method(
    method: CeilingMethod,
    class: AssetClass,
    market: Market,
) -> Result<(), String> {
    match ReferenceFamily::of(class, market) {
        Some(family) if family.accepts(method) => Ok(()),
        Some(family) => {
            let accepted: Vec<&str> = family.methods().iter().map(|m| m.as_str()).collect();
            Err(format!(
                "O teto pelo metodo {} nao vale para {}: use {}, um valor proprio ou nenhum.",
                method.as_str(),
                class.label(),
                accepted.join(", ")
            ))
        }
        None => Err(format!(
            "{} nao tem referencia de preco: o teto so pode ser um valor proprio ou nenhum.",
            class.label()
        )),
    }
}

/// O que a pessoa escolheu como teto.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CeilingChoice {
    #[default]
    None,
    Method(CeilingMethod),
    /// Valor próprio, fixo, em reais por unidade.
    Manual(Decimal),
}

impl CeilingChoice {
    pub fn method(self) -> Option<CeilingMethod> {
        match self {
            CeilingChoice::Method(method) => Some(method),
            _ => None,
        }
    }

    pub fn manual(self) -> Option<Decimal> {
        match self {
            CeilingChoice::Manual(value) => Some(value),
            _ => None,
        }
    }
}

/// De onde veio o valor do teto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CeilingSource {
    Manual,
    Method(CeilingMethod),
}

/// O teto que vale hoje.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Ceiling {
    #[default]
    None,
    /// Acima deste preço por unidade, o ativo fica fora do aporte.
    Price {
        source: CeilingSource,
        value: Decimal,
    },
    /// A pessoa escolheu um método que hoje está sem valor: o ativo fica fora
    /// do aporte até o método voltar a ter valor (ou ela trocar a escolha).
    Unavailable {
        method: CeilingMethod,
        reason: ReferenceGap,
    },
}

impl Ceiling {
    /// O preço do teto, quando há.
    pub fn value(self) -> Option<Decimal> {
        match self {
            Ceiling::Price { value, .. } => Some(value),
            _ => None,
        }
    }
}

/// As referências de um ativo, pela família.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetReferences {
    Stock(PriceReferences),
    Fund(FundReferences),
}

impl AssetReferences {
    pub fn family(&self) -> ReferenceFamily {
        match self {
            AssetReferences::Stock(_) => ReferenceFamily::Stock,
            AssetReferences::Fund(_) => ReferenceFamily::Fund,
        }
    }

    /// O valor de hoje de um método, ou o motivo de não haver. Método de outra
    /// família não tem valor ([`ReferenceGap::NoReference`]).
    pub fn value(&self, method: CeilingMethod) -> Result<Decimal, ReferenceGap> {
        match (self, method) {
            (AssetReferences::Stock(r), CeilingMethod::Bazin) => r.bazin.map(|b| b.price),
            (AssetReferences::Stock(r), CeilingMethod::Graham) => r.graham,
            (AssetReferences::Stock(r), CeilingMethod::Combined) => r.combined(),
            (AssetReferences::Fund(r), CeilingMethod::Nav) => r.nav,
            (AssetReferences::Fund(r), CeilingMethod::FundYield) => r.fund_yield.map(|y| y.price),
            (AssetReferences::Fund(r), CeilingMethod::Combined) => r.combined(),
            _ => Err(ReferenceGap::NoReference),
        }
    }
}

/// O teto de hoje pela escolha e pelas referências do ativo. É a **única**
/// conta do teto: o motor do aporte, a Análise, os chips do plano, a carteira
/// e os alertas passam por ela.
pub fn effective(choice: CeilingChoice, references: Option<&AssetReferences>) -> Ceiling {
    match choice {
        CeilingChoice::None => Ceiling::None,
        CeilingChoice::Manual(value) => Ceiling::Price {
            source: CeilingSource::Manual,
            value,
        },
        CeilingChoice::Method(method) => {
            match references.map_or(Err(ReferenceGap::NoReference), |r| r.value(method)) {
                Ok(value) => Ceiling::Price {
                    source: CeilingSource::Method(method),
                    value,
                },
                Err(reason) => Ceiling::Unavailable { method, reason },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::analysis::references::Bazin;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    fn stock(bazin: Option<Decimal>, graham: Option<Decimal>) -> AssetReferences {
        AssetReferences::Stock(PriceReferences {
            bazin: bazin
                .map(|price| Bazin {
                    annual_average: price / dec!(10),
                    years: 1,
                    price,
                })
                .ok_or(ReferenceGap::NoDividends),
            graham: graham.ok_or(ReferenceGap::Loss),
        })
    }

    #[test]
    fn methods_round_trip_and_belong_to_a_family() {
        for method in CeilingMethod::ALL {
            assert_eq!(CeilingMethod::parse(method.as_str()), Some(method));
            assert_eq!(
                serde_json::to_value(method).unwrap(),
                serde_json::Value::String(method.as_str().to_owned())
            );
        }
        assert_eq!(CeilingMethod::parse("manual"), None);
        assert_eq!(
            ReferenceFamily::of(AssetClass::BrStocks, Market::B3),
            Some(ReferenceFamily::Stock)
        );
        assert_eq!(
            ReferenceFamily::of(AssetClass::BrReits, Market::B3),
            Some(ReferenceFamily::Fund)
        );
        for (class, market) in [
            (AssetClass::IntlStocks, Market::Us),
            (AssetClass::IntlStocks, Market::B3),
            (AssetClass::Crypto, Market::Crypto),
            (AssetClass::FixedIncome, Market::Treasury),
            (AssetClass::UsReits, Market::Us),
        ] {
            assert_eq!(ReferenceFamily::of(class, market), None);
        }
    }

    #[test]
    fn a_method_of_another_family_is_refused() {
        let b3 = Market::B3;
        assert!(check_method(CeilingMethod::Graham, AssetClass::BrStocks, b3).is_ok());
        assert!(check_method(CeilingMethod::Combined, AssetClass::BrReits, b3).is_ok());
        let graham_in_fii =
            check_method(CeilingMethod::Graham, AssetClass::BrReits, b3).unwrap_err();
        assert!(graham_in_fii.contains("graham"), "{graham_in_fii}");
        assert!(
            graham_in_fii.contains("nav, fund_yield, combined"),
            "{graham_in_fii}"
        );
        assert!(graham_in_fii.is_ascii(), "{graham_in_fii}");
        assert!(check_method(CeilingMethod::Nav, AssetClass::BrStocks, b3).is_err());
        let crypto =
            check_method(CeilingMethod::Bazin, AssetClass::Crypto, Market::Crypto).unwrap_err();
        assert!(crypto.contains("valor proprio"), "{crypto}");
        assert!(check_method(CeilingMethod::Bazin, AssetClass::IntlStocks, b3).is_err());
    }

    #[test]
    fn the_effective_ceiling_follows_the_method() {
        let refs = stock(Some(dec!(18.69)), Some(dec!(77.77)));
        assert_eq!(effective(CeilingChoice::None, Some(&refs)), Ceiling::None);
        assert_eq!(
            effective(CeilingChoice::Manual(dec!(30)), None),
            Ceiling::Price {
                source: CeilingSource::Manual,
                value: dec!(30)
            }
        );
        assert_eq!(
            effective(CeilingChoice::Method(CeilingMethod::Graham), Some(&refs)).value(),
            Some(dec!(77.77))
        );
        assert_eq!(
            effective(CeilingChoice::Method(CeilingMethod::Combined), Some(&refs)).value(),
            Some(dec!(18.69))
        );
    }

    #[test]
    fn a_method_without_value_is_unavailable_with_the_reason_never_no_ceiling() {
        // Prejuízo: o Graham some e o teto pelo Graham fica sem valor.
        let loss = stock(Some(dec!(18.69)), None);
        let ceiling = effective(CeilingChoice::Method(CeilingMethod::Graham), Some(&loss));
        assert_eq!(
            ceiling,
            Ceiling::Unavailable {
                method: CeilingMethod::Graham,
                reason: ReferenceGap::Loss
            }
        );
        assert_eq!(ceiling.value(), None);
        // "Pelas duas" com uma só é ela.
        assert_eq!(
            effective(CeilingChoice::Method(CeilingMethod::Combined), Some(&loss)).value(),
            Some(dec!(18.69))
        );
        // Sem referências carregadas, nem de outra família, nunca vira "sem teto".
        assert_eq!(
            effective(CeilingChoice::Method(CeilingMethod::Nav), Some(&loss)),
            Ceiling::Unavailable {
                method: CeilingMethod::Nav,
                reason: ReferenceGap::NoReference
            }
        );
        assert!(matches!(
            effective(CeilingChoice::Method(CeilingMethod::Bazin), None),
            Ceiling::Unavailable { .. }
        ));
    }

    proptest! {
        /// Escolha de método nunca dá `Ceiling::None`: ou há valor, ou há motivo.
        #[test]
        fn a_method_choice_is_never_silently_no_ceiling(
            bazin in prop::option::of(1i64..100_000),
            graham in prop::option::of(1i64..100_000),
            method in prop::sample::select(CeilingMethod::ALL.to_vec()),
            loaded in any::<bool>(),
        ) {
            let refs = stock(bazin.map(|c| Decimal::new(c, 2)), graham.map(|c| Decimal::new(c, 2)));
            let ceiling = effective(CeilingChoice::Method(method), loaded.then_some(&refs));
            prop_assert!(ceiling != Ceiling::None);
            if let Ceiling::Price { source, value } = ceiling {
                prop_assert_eq!(source, CeilingSource::Method(method));
                prop_assert!(value > Decimal::ZERO);
            }
        }
    }
}
