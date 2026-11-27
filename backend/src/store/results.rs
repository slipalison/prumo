//! Resultados trimestrais da carteira (migração 0031): os trimestres de cada
//! companhia, a partir dos documentos da CVM que os fundamentos já gravam e da
//! data de entrega que a Agenda lê do índice dos arquivos
//! (`cvm_filing_receipts`, migração 0028), e a série mensal de cada FII. A
//! conta é a regra pura de `domain::fundamentals::quarters` e `fund_months`,
//! feita na hora da leitura.

use std::collections::HashMap;

use rust_decimal::Decimal;
use sqlx::FromRow;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::decode_error;
use super::{Store, StoreError};
use crate::domain::fundamentals::{
    fund_months, quarterly_results, AssetFundamentals, FilingKind, FundMonth, FundMonthFigures,
    QuarterlyResults, Receipts, Template, Unsupported,
};

/// Quantos meses a série do FII mostra.
pub const FUND_MONTHS: usize = 12;

/// Os resultados de uma companhia da carteira.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyResults {
    pub name: String,
    pub template: Template,
    /// Ver `CompanyMetrics::holding`: na holding a margem não se aplica.
    pub holding: bool,
    pub results: QuarterlyResults,
    pub synced_at: Option<OffsetDateTime>,
}

/// A série mensal de um FII da carteira.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundResults {
    pub name: String,
    /// Até [`FUND_MONTHS`] meses, do mais velho ao mais novo.
    pub months: Vec<FundMonthFigures>,
    pub synced_at: Option<OffsetDateTime>,
}

/// Os resultados de um ativo da carteira, por símbolo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetResults {
    Company(Box<CompanyResults>),
    Fund(Box<FundResults>),
    Unsupported(Unsupported),
}

#[derive(FromRow)]
struct ReceiptRow {
    cvm_code: i32,
    kind: String,
    reference_date: Date,
    received_on: Date,
}

#[derive(FromRow)]
struct MonthRow {
    cnpj: String,
    reference_date: Date,
    version: i32,
    net_assets: Option<Decimal>,
    shares: Option<Decimal>,
    dividend_yield: Option<Decimal>,
}

impl Store {
    /// Os resultados dos ativos da carteira da pessoa, por símbolo, com a
    /// mesma cobertura dos fundamentos (ETF, BDR, fora da B3 e o que ainda não
    /// foi buscado vêm como [`AssetResults::Unsupported`] com o motivo).
    pub async fn results_for_user(
        &self,
        user: Uuid,
    ) -> Result<HashMap<String, AssetResults>, StoreError> {
        let fundamentals = self.fundamentals_for_user(user).await?;
        let codes: Vec<i32> = fundamentals
            .values()
            .filter_map(|f| match f {
                AssetFundamentals::Company(m) => Some(m.cvm_code),
                _ => None,
            })
            .collect();
        let cnpjs: Vec<String> = fundamentals
            .values()
            .filter_map(|f| match f {
                AssetFundamentals::Fund(m) => Some(m.cnpj.clone()),
                _ => None,
            })
            .collect();
        let (filings, receipts) = if codes.is_empty() {
            (HashMap::new(), HashMap::new())
        } else {
            (
                self.company_filings(&codes).await?,
                self.receipts(&codes).await?,
            )
        };
        let no_receipts = Receipts::new();
        let months = self.fund_month_rows(&cnpjs).await?;
        Ok(fundamentals
            .into_iter()
            .map(|(symbol, found)| {
                let entry = match found {
                    AssetFundamentals::Company(m) => {
                        let docs = filings
                            .get(&m.cvm_code)
                            .map(Vec::as_slice)
                            .unwrap_or_default();
                        let received = receipts.get(&m.cvm_code).unwrap_or(&no_receipts);
                        AssetResults::Company(Box::new(CompanyResults {
                            results: quarterly_results(docs, received, m.holding),
                            name: m.name,
                            template: m.template,
                            holding: m.holding,
                            synced_at: m.synced_at,
                        }))
                    }
                    AssetFundamentals::Fund(m) => {
                        let rows = months.get(&m.cnpj).map(Vec::as_slice).unwrap_or_default();
                        AssetResults::Fund(Box::new(FundResults {
                            months: fund_months(rows, FUND_MONTHS),
                            name: m.name,
                            synced_at: m.synced_at,
                        }))
                    }
                    AssetFundamentals::Unsupported(reason) => AssetResults::Unsupported(reason),
                };
                (symbol, entry)
            })
            .collect())
    }

    /// A data em que a CVM recebeu cada DFP e ITR das companhias `codes`, por
    /// companhia (a versão mais alta que a Agenda leu).
    async fn receipts(&self, codes: &[i32]) -> Result<HashMap<i32, Receipts>, StoreError> {
        let rows: Vec<ReceiptRow> = sqlx::query_as(
            "select cvm_code, kind, reference_date, received_on
               from cvm_filing_receipts where cvm_code = any($1)",
        )
        .bind(codes)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<i32, Receipts> = HashMap::new();
        for row in rows {
            let kind = FilingKind::parse(&row.kind).ok_or_else(|| decode_error(&row.kind))?;
            out.entry(row.cvm_code)
                .or_default()
                .insert((kind, row.reference_date), row.received_on);
        }
        Ok(out)
    }

    async fn fund_month_rows(
        &self,
        cnpjs: &[String],
    ) -> Result<HashMap<String, Vec<FundMonth>>, StoreError> {
        if cnpjs.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<MonthRow> = sqlx::query_as(
            "select cnpj, reference_date, version, net_assets, shares, dividend_yield
               from cvm_fund_months
              where cnpj = any($1)
              order by cnpj, reference_date",
        )
        .bind(cnpjs)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<String, Vec<FundMonth>> = HashMap::new();
        for row in rows {
            out.entry(row.cnpj.clone()).or_default().push(FundMonth {
                cnpj: row.cnpj,
                reference_date: row.reference_date,
                version: row.version,
                net_assets: row.net_assets.map(|d| d.normalize()),
                shares: row.shares.map(|d| d.normalize()),
                dividend_yield: row.dividend_yield.map(|d| d.normalize()),
            });
        }
        Ok(out)
    }
}
