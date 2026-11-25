//! A parte do Tesouro Direto na busca de símbolos: os títulos vigentes que
//! combinam com o que a pessoa digitou.

use time::OffsetDateTime;

use super::dto::SymbolDto;
use crate::domain::asset_class::AssetClass;
use crate::domain::treasury::{matches_query, Bond, VENUE};
use crate::store::{Store, StoreError};
use crate::treasury::today_in_brasilia;

/// Quantos títulos a busca sugere de uma vez. "tesouro" sozinho casa com todos
/// (uns 60); a lista corta no que cabe na tela, e a pessoa afina digitando
/// "ipca 2035".
const MAX_SUGGESTIONS: usize = 10;

/// Os títulos vigentes cujo nome combina com `term`, do tipo mais simples para o
/// mais longo e, dentro do tipo, do vencimento mais próximo ao mais distante.
pub async fn suggestions(store: &Store, term: &str) -> Result<Vec<SymbolDto>, StoreError> {
    let today = today_in_brasilia(OffsetDateTime::now_utc());
    let mut found: Vec<(Bond, SymbolDto)> = store
        .treasury_catalog()
        .await?
        .into_iter()
        .filter_map(|quote| {
            let bond = Bond::from_symbol(&quote.symbol)?;
            // Venceu: o preço guardado é de um título que ninguém compra mais.
            if bond.maturity < today {
                return None;
            }
            let name = quote.name.unwrap_or_else(|| bond.name());
            matches_query(term, &name).then(|| {
                (
                    bond,
                    SymbolDto {
                        symbol: quote.symbol,
                        display_name: name,
                        exchange: VENUE.to_owned(),
                        suggested_class: AssetClass::FixedIncome,
                        listed_on_b3: false,
                        treasury: true,
                    },
                )
            })
        })
        .collect();
    found.sort_by_key(|(bond, _)| *bond);
    Ok(found
        .into_iter()
        .take(MAX_SUGGESTIONS)
        .map(|(_, dto)| dto)
        .collect())
}
