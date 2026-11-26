//! O aviso do aporte para ativo sem teto de preço: as referências públicas de
//! Bazin e de Graham ao lado do preço de hoje. É só informação (o teto segue
//! opcional, nada bloqueia e o plano não muda); a conta é a mesma da Análise
//! (`crate::analysis::price_references`). Contrato em `docs/api.md`
//! (`POST /plans`, campo `price_references`).

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use super::dto::{decimal_out, PlanDto};
use crate::domain::analysis::references::PriceReferences;

/// As referências públicas de preço de um ativo sem teto. Não é recomendação
/// nem preço justo; a decisão de aportar é da pessoa.
#[derive(Debug, Serialize, PartialEq)]
pub struct PriceReferencesDto {
    #[serde(with = "decimal_out::option")]
    pub bazin: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub graham: Option<Decimal>,
}

impl From<&PriceReferences> for PriceReferencesDto {
    fn from(references: &PriceReferences) -> Self {
        Self {
            bazin: references.bazin.map(|b| b.price),
            graham: references.graham,
        }
    }
}

impl PlanDto {
    /// Põe em cada item as referências de preço dele
    /// (`crate::analysis::plan_references`). Só informa: nenhum número do
    /// plano muda.
    pub fn with_price_references(mut self, found: &HashMap<Uuid, PriceReferences>) -> Self {
        for item in &mut self.items {
            item.price_references = found.get(&item.asset_id).map(PriceReferencesDto::from);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::analysis::references::Bazin;
    use rust_decimal_macros::dec;

    #[test]
    fn the_dto_carries_each_reference_that_exists() {
        let both = PriceReferences {
            bazin: Some(Bazin {
                annual_average: dec!(1.12),
                years: 3,
                price: dec!(18.69),
            }),
            graham: Some(dec!(77.77)),
        };
        assert_eq!(
            serde_json::to_value(PriceReferencesDto::from(&both)).unwrap(),
            serde_json::json!({ "bazin": 18.69, "graham": 77.77 })
        );
        let graham_only = PriceReferences {
            bazin: None,
            graham: Some(dec!(20.12)),
        };
        assert_eq!(
            serde_json::to_value(PriceReferencesDto::from(&graham_only)).unwrap(),
            serde_json::json!({ "bazin": null, "graham": 20.12 })
        );
    }
}
