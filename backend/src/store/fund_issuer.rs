//! Ticker de FII → fundo (CNPJ): o que a B3 disse (`b3_fund_cnpj`, migração
//! 0036) e os fundos que declaram o emissor no informe mensal (`cvm_funds`,
//! com a série de `cvm_fund_months`). A regra é [`choose_fund`]; o método, em
//! `docs/fundamentos.md` ("Ticker de FII → fundo").
//!
//! [`Store::fund_cnpjs`] é o caminho único: os fundamentos (valor patrimonial,
//! P/VP e, por eles, a série mensal dos resultados), o CNPJ do Fundos.NET da
//! Agenda e o CNPJ da fonte pagadora do IR saem dele.

use std::collections::HashMap;

use time::{Date, OffsetDateTime};

use super::{Store, StoreError};
use crate::domain::fund_issuer::{choose_fund, FundChoice, FundClaim};
use crate::fundamentals::b3::ListedFund;

/// O que a B3 disse do fundo de uma sigla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct B3Fund {
    /// `None`: a B3 não lista o fundo.
    pub cnpj: Option<String>,
    pub name: Option<String>,
    /// `None`: ainda não perguntado (só reservado).
    pub fetched_at: Option<OffsetDateTime>,
}

/// Uma linha de `b3_fund_cnpj`: sigla, CNPJ, nome e quando foi perguntado.
type B3FundRow = (
    String,
    Option<String>,
    Option<String>,
    Option<OffsetDateTime>,
);

fn pg_interval(duration: time::Duration) -> String {
    format!("{} seconds", duration.whole_seconds())
}

impl Store {
    /// O que a B3 disse de cada sigla.
    pub async fn b3_funds(
        &self,
        issuers: &[String],
    ) -> Result<HashMap<String, B3Fund>, StoreError> {
        if issuers.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<B3FundRow> = sqlx::query_as(
            "select issuer, cnpj, name, fetched_at from b3_fund_cnpj where issuer = any($1)",
        )
        .bind(issuers)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(issuer, cnpj, name, fetched_at)| {
                (
                    issuer,
                    B3Fund {
                        cnpj,
                        name,
                        fetched_at,
                    },
                )
            })
            .collect())
    }

    /// Reserva a busca do fundo de uma sigla na B3. `false`: outra réplica
    /// está buscando.
    pub async fn claim_b3_fund(
        &self,
        issuer: &str,
        now: OffsetDateTime,
        lease: time::Duration,
    ) -> Result<bool, StoreError> {
        let claimed: Option<(String,)> = sqlx::query_as(
            "insert into b3_fund_cnpj as f (issuer, claimed_until)
             values ($1, $2::timestamptz + $3::interval)
             on conflict (issuer) do update set claimed_until = excluded.claimed_until
              where f.claimed_until is null or f.claimed_until <= $2::timestamptz
             returning issuer",
        )
        .bind(issuer)
        .bind(now)
        .bind(pg_interval(lease))
        .fetch_optional(&self.pool)
        .await?;
        Ok(claimed.is_some())
    }

    /// Grava o que a B3 disse (`None`: não lista o fundo) e solta a reserva.
    pub async fn save_b3_fund(
        &self,
        issuer: &str,
        fund: Option<&ListedFund>,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into b3_fund_cnpj (issuer, cnpj, name, fetched_at)
             values ($1, $2, $3, $4)
             on conflict (issuer) do update set
                cnpj = excluded.cnpj, name = excluded.name,
                fetched_at = excluded.fetched_at, claimed_until = null",
        )
        .bind(issuer)
        .bind(fund.map(|f| f.cnpj.as_str()))
        .bind(fund.map(|f| f.name.as_str()))
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn release_b3_fund(&self, issuer: &str) -> Result<(), StoreError> {
        sqlx::query("update b3_fund_cnpj set claimed_until = null where issuer = $1")
            .bind(issuer)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Os fundos que declaram cada emissor no informe mensal: o CNPJ, o
    /// primeiro mês conhecido (da série mensal, ou o do informe) e o mês do
    /// informe mais novo.
    pub async fn fund_claims(
        &self,
        issuers: &[String],
    ) -> Result<HashMap<String, Vec<FundClaim>>, StoreError> {
        if issuers.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(String, String, Date, Date)> = sqlx::query_as(
            "select f.issuer, f.cnpj,
                    least(f.reference_date,
                          coalesce((select min(m.reference_date) from cvm_fund_months m
                                     where m.cnpj = f.cnpj), f.reference_date)),
                    f.reference_date
               from cvm_funds f
              where f.issuer = any($1)
              order by f.issuer, f.cnpj",
        )
        .bind(issuers)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<String, Vec<FundClaim>> = HashMap::new();
        for (issuer, cnpj, first_reported, last_reported) in rows {
            out.entry(issuer).or_default().push(FundClaim {
                cnpj,
                first_reported,
                last_reported,
            });
        }
        Ok(out)
    }

    /// O fundo (CNPJ, como a CVM escreve) de cada emissor de FII: o que a B3
    /// disse, ou o desempate entre os que declaram o emissor
    /// ([`choose_fund`]). Emissor sem nenhum dos dois não volta.
    pub async fn fund_cnpjs(
        &self,
        issuers: &[String],
    ) -> Result<HashMap<String, FundChoice>, StoreError> {
        let b3 = self.b3_funds(issuers).await?;
        let claims = self.fund_claims(issuers).await?;
        Ok(issuers
            .iter()
            .filter_map(|issuer| {
                let said = b3.get(issuer).and_then(|f| f.cnpj.as_deref());
                let found = claims.get(issuer).map(Vec::as_slice).unwrap_or_default();
                choose_fund(found, said).map(|choice| (issuer.clone(), choice))
            })
            .collect())
    }
}
