//! O aviso do aporte para ativo sem teto de preço: as referências públicas do
//! ativo ao lado do preço de hoje — Bazin e Graham nas ações, VP e Rendimento
//! nos FIIs. É só informação (o teto segue opcional, nada bloqueia e o plano
//! não muda); a conta é a mesma da Análise (`crate::ceiling`). Contrato em
//! `docs/api.md` (`POST /plans`, campo `price_references`).

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use super::dto::{decimal_out, PlanDto};
use crate::domain::ceiling::{AssetReferences, CeilingMethod};

/// Uma referência pública de preço de um ativo sem teto. Não é recomendação
/// nem preço justo; a decisão de aportar é da pessoa.
#[derive(Debug, Serialize, PartialEq)]
pub struct PriceReferenceDto {
    pub method: CeilingMethod,
    #[serde(with = "decimal_out")]
    pub value: Decimal,
}

/// As referências que têm valor, uma por método da família ("Pelas duas" é a
/// menor delas e não vira chip próprio). `None` sem nenhuma.
pub fn chips(references: &AssetReferences) -> Option<Vec<PriceReferenceDto>> {
    let found: Vec<PriceReferenceDto> = references
        .family()
        .methods()
        .into_iter()
        .filter(|m| *m != CeilingMethod::Combined)
        .filter_map(|method| {
            let value = references.value(method).ok()?;
            Some(PriceReferenceDto { method, value })
        })
        .collect();
    (!found.is_empty()).then_some(found)
}

impl PlanDto {
    /// Põe em cada item as referências de preço dele
    /// (`crate::ceiling::plan_references`). Só informa: nenhum número do plano
    /// muda.
    pub fn with_price_references(mut self, found: &HashMap<Uuid, AssetReferences>) -> Self {
        for item in &mut self.items {
            item.price_references = found.get(&item.asset_id).and_then(chips);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::analysis::fund_references::FundReferences;
    use crate::domain::analysis::references::{Bazin, PriceReferences, ReferenceGap};
    use rust_decimal_macros::dec;

    #[test]
    fn the_chips_carry_each_reference_that_exists() {
        let both = AssetReferences::Stock(PriceReferences {
            bazin: Ok(Bazin {
                annual_average: dec!(1.12),
                years: 3,
                price: dec!(18.69),
            }),
            graham: Ok(dec!(77.77)),
        });
        assert_eq!(
            serde_json::to_value(chips(&both)).unwrap(),
            serde_json::json!([
                { "method": "bazin", "value": 18.69 },
                { "method": "graham", "value": 77.77 }
            ])
        );
        let graham_only = AssetReferences::Stock(PriceReferences {
            bazin: Err(ReferenceGap::NoDividends),
            graham: Ok(dec!(20.12)),
        });
        assert_eq!(
            serde_json::to_value(chips(&graham_only)).unwrap(),
            serde_json::json!([{ "method": "graham", "value": 20.12 }])
        );
        let fund = AssetReferences::Fund(FundReferences {
            fund_type: None,
            composition: None,
            nav: Ok(dec!(98.1)),
            fund_yield: Err(ReferenceGap::UnknownFundType),
        });
        assert_eq!(
            serde_json::to_value(chips(&fund)).unwrap(),
            serde_json::json!([{ "method": "nav", "value": 98.1 }])
        );
        let none = AssetReferences::Stock(PriceReferences {
            bazin: Err(ReferenceGap::NoDividends),
            graham: Err(ReferenceGap::Loss),
        });
        assert_eq!(chips(&none), None);
    }
}
