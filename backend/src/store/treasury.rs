//! O catálogo do Tesouro Direto no banco. Não tem tabela própria: o título
//! vigente é uma linha de `quotes` com o símbolo `TD-<tipo>-<vencimento>` —
//! o nome do título em `name`, o preço de resgate em `price` e a data-base da
//! tabela em `traded_at`. Assim a cotação do título entra na carteira pelo
//! mesmo caminho das outras.

use sqlx::FromRow;
use time::OffsetDateTime;

use super::{Store, StoreError, StoredQuote};

/// Padrão do `like` das linhas de título: o prefixo do símbolo (conferido por
/// teste contra `domain::treasury::SYMBOL_PREFIX`) e o curinga.
const LIKE: &str = "TD-%";

#[derive(FromRow)]
struct CatalogRow {
    symbol: String,
    price: rust_decimal::Decimal,
    currency: String,
    name: Option<String>,
    fetched_at: OffsetDateTime,
    traded_at: Option<OffsetDateTime>,
}

/// Quantos dias de folga a data-base de um título pode ter em relação ao mais
/// novo da tabela para ele ainda contar como vigente. A tabela traz só o que o
/// Tesouro oferece; um título que sumiu dela fica parado na data antiga.
const OFFERED_WITHIN_DAYS: i32 = 7;

impl Store {
    /// Os títulos que o Tesouro está oferecendo (os que aparecem na tabela mais
    /// recente), do símbolo menor para o maior. Título que a pessoa guarda e que
    /// saiu da tabela não entra aqui: não é sugestão para quem cadastra.
    pub async fn treasury_catalog(&self) -> Result<Vec<StoredQuote>, StoreError> {
        let rows: Vec<CatalogRow> = sqlx::query_as(
            "select symbol, price, currency, name, fetched_at, traded_at from quotes
              where symbol like $1
                and traded_at >= (select max(traded_at) from quotes where symbol like $1)
                                 - make_interval(days => $2)
              order by symbol",
        )
        .bind(LIKE)
        .bind(OFFERED_WITHIN_DAYS)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| StoredQuote {
                symbol: r.symbol,
                price: r.price.normalize(),
                currency: r.currency,
                name: r.name,
                fetched_at: r.fetched_at,
                traded_at: r.traded_at,
            })
            .collect())
    }

    /// Quando a tabela do Tesouro foi gravada pela última vez; `None` se nunca.
    pub async fn last_treasury_fetch(&self) -> Result<Option<OffsetDateTime>, StoreError> {
        let (at,): (Option<OffsetDateTime>,) =
            sqlx::query_as("select max(fetched_at) from quotes where symbol like $1")
                .bind(LIKE)
                .fetch_one(&self.pool)
                .await?;
        Ok(at)
    }

    /// Apaga as cotações de título que não vieram na tabela de hoje (venceram
    /// ou saíram de oferta), menos as dos títulos que alguma carteira guarda:
    /// essas ficam com o último preço e a data dele. Devolve quantas apagou.
    pub async fn prune_treasury(&self, keep: &[String]) -> Result<u64, StoreError> {
        let done = sqlx::query(
            "delete from quotes q
              where q.symbol like $1
                and q.symbol <> all($2)
                and not exists (select 1 from assets a where a.treasury and a.symbol = q.symbol)",
        )
        .bind(LIKE)
        .bind(keep)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::LIKE;
    use crate::domain::treasury::SYMBOL_PREFIX;

    #[test]
    fn the_like_pattern_is_the_symbol_prefix() {
        assert_eq!(LIKE, format!("{SYMBOL_PREFIX}%"));
    }
}
