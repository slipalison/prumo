//! Fundamentos no Postgres (migração 0020): a referência lida da CVM
//! (cadastro, documentos das companhias da carteira, informes de FII), o
//! código CVM de cada emissor da B3 e o estado de cada arquivo.
//!
//! Toda escrita é uma transação curta, sob `pg_advisory_xact_lock` do arquivo:
//! nenhuma conexão fica presa durante o download (o pool é de quatro conexões
//! por réplica). A vez de cada réplica é uma reserva no banco
//! (`claimed_until`), como nos proventos automáticos.
//!
//! A leitura ([`Store::fundamentals_for_user`]) faz a conta na hora, com a
//! regra pura de `domain::fundamentals`: mudar a regra não pede baixar de novo.

use std::collections::{BTreeSet, HashMap, HashSet};

use rust_decimal::Decimal;
use sqlx::{FromRow, Postgres, Transaction};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::{decode_error, Asset, Store, StoreError};
use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::fundamentals::{
    company_metrics, coverage, fund_metrics, AssetFundamentals, Column, Coverage, Figure, Filing,
    FilingKind, FundMonth, FundReport, Gap, Line, Registration, Scope, Statement, Unsupported,
    Vacancy,
};
use crate::fundamentals::b3::IssuerInfo;
use crate::fundamentals::download::Validators;

/// Os emissores que alguém tem na carteira (todas as pessoas).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wanted {
    pub companies: BTreeSet<String>,
    pub funds: BTreeSet<String>,
}

/// O código CVM de um emissor, como a B3 informou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IssuerCode {
    pub cvm_code: Option<i32>,
    /// Total de ações emitidas, da B3.
    pub total_shares: Option<Decimal>,
    pub fetched_at: Option<OffsetDateTime>,
}

/// O que se sabe de um arquivo da CVM.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceState {
    pub validators: Validators,
    /// Os códigos CVM que a versão gravada já teve lidos.
    pub scanned: BTreeSet<i32>,
    pub attempted_at: Option<OffsetDateTime>,
    /// A última leitura que deu certo (200 lido ou 304).
    pub checked_at: Option<OffsetDateTime>,
    pub last_error: Option<String>,
}

/// A vacância de um fundo, pronta para gravar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundVacancy {
    pub cnpj: String,
    pub vacancy: Vacancy,
}

fn pg_interval(duration: time::Duration) -> String {
    format!("{} seconds", duration.whole_seconds())
}

pub(super) async fn lock(tx: &mut Transaction<'_, Postgres>, key: &str) -> Result<(), StoreError> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(format!("fundamentals:{key}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Marca o arquivo como lido com sucesso agora, com esta versão.
pub(super) async fn mark_checked(
    tx: &mut Transaction<'_, Postgres>,
    key: &str,
    validators: &Validators,
    scanned: &BTreeSet<i32>,
    now: OffsetDateTime,
) -> Result<(), StoreError> {
    let scanned: Vec<i32> = scanned.iter().copied().collect();
    sqlx::query(
        "insert into cvm_sources (source, etag, last_modified, scanned_codes, attempted_at, checked_at)
         values ($1, $2, $3, $4, $5, $5)
         on conflict (source) do update set
            etag = excluded.etag,
            last_modified = excluded.last_modified,
            scanned_codes = excluded.scanned_codes,
            attempted_at = excluded.attempted_at,
            checked_at = excluded.checked_at,
            last_error = null,
            claimed_until = null",
    )
    .bind(key)
    .bind(validators.etag.as_deref())
    .bind(validators.last_modified.as_deref())
    .bind(&scanned)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

type IssuerRow = (String, Option<i32>, Option<Decimal>, Option<OffsetDateTime>);

type SourceRow = (
    String,
    Option<String>,
    Option<String>,
    Vec<i32>,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
    Option<String>,
);

fn source_state(
    (source, etag, last_modified, scanned, attempted_at, checked_at, last_error): SourceRow,
) -> (String, SourceState) {
    (
        source,
        SourceState {
            validators: Validators {
                etag,
                last_modified,
            },
            scanned: scanned.into_iter().collect(),
            attempted_at,
            checked_at,
            last_error,
        },
    )
}

#[derive(FromRow)]
struct FilingRow {
    cvm_code: i32,
    kind: String,
    reference_date: Date,
    version: i32,
    scope: String,
    company_name: String,
}

#[derive(FromRow)]
struct LineRow {
    cvm_code: i32,
    kind: String,
    reference_date: Date,
    statement: String,
    period: String,
    period_start: Option<Date>,
    period_end: Date,
    code: String,
    description: String,
    value: Decimal,
}

#[derive(FromRow)]
struct RegistrationRow {
    cvm_code: i32,
    cnpj: String,
    name: String,
    status: String,
    issuer_status: Option<String>,
    sector: Option<String>,
}

#[derive(FromRow)]
struct FundRow {
    cnpj: String,
    issuer: String,
    isin: String,
    name: String,
    reference_date: Date,
    version: i32,
    net_assets: Option<Decimal>,
    shares: Option<Decimal>,
    vacancy_reference_date: Option<Date>,
    vacancy: Option<Decimal>,
    vacancy_gap: Option<String>,
    properties: Option<i32>,
}

#[derive(FromRow)]
struct HeldRow {
    asset_class: String,
    symbol: String,
    listed_on_b3: bool,
    treasury: bool,
}

fn coverage_of(
    class: AssetClass,
    listed_on_b3: bool,
    treasury: bool,
    symbol: Option<&str>,
) -> Coverage {
    coverage(class, class.market_of(listed_on_b3, treasury), symbol)
}

impl Store {
    /// Os emissores de companhia e de FII de todas as carteiras.
    pub async fn fundamentals_wanted(&self) -> Result<Wanted, StoreError> {
        let rows: Vec<HeldRow> = sqlx::query_as(
            "select distinct asset_class, symbol, listed_on_b3, treasury
               from assets where symbol is not null",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut wanted = Wanted::default();
        for row in rows {
            let class = AssetClass::parse(&row.asset_class)
                .ok_or_else(|| decode_error(&row.asset_class))?;
            match coverage_of(class, row.listed_on_b3, row.treasury, Some(&row.symbol)) {
                Coverage::Company { issuer } => {
                    wanted.companies.insert(issuer);
                }
                Coverage::Fund { issuer } => {
                    wanted.funds.insert(issuer);
                }
                Coverage::Unsupported(_) => {}
            }
        }
        Ok(wanted)
    }

    pub async fn issuer_codes(
        &self,
        issuers: &[String],
    ) -> Result<HashMap<String, IssuerCode>, StoreError> {
        let rows: Vec<IssuerRow> = sqlx::query_as(
            "select issuer, cvm_code, total_shares, fetched_at from b3_issuer_cvm where issuer = any($1)",
        )
        .bind(issuers)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(issuer, cvm_code, total_shares, fetched_at)| {
                (
                    issuer,
                    IssuerCode {
                        cvm_code,
                        total_shares: total_shares.map(|s| s.normalize()),
                        fetched_at,
                    },
                )
            })
            .collect())
    }

    /// Reserva a busca do código de um emissor. `false`: outra réplica está
    /// buscando.
    pub async fn claim_issuer_code(
        &self,
        issuer: &str,
        now: OffsetDateTime,
        lease: time::Duration,
    ) -> Result<bool, StoreError> {
        let claimed: Option<(String,)> = sqlx::query_as(
            "insert into b3_issuer_cvm as c (issuer, claimed_until)
             values ($1, $2::timestamptz + $3::interval)
             on conflict (issuer) do update set claimed_until = excluded.claimed_until
              where c.claimed_until is null or c.claimed_until <= $2::timestamptz
             returning issuer",
        )
        .bind(issuer)
        .bind(now)
        .bind(pg_interval(lease))
        .fetch_optional(&self.pool)
        .await?;
        Ok(claimed.is_some())
    }

    pub async fn save_issuer_code(
        &self,
        issuer: &str,
        info: &IssuerInfo,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into b3_issuer_cvm (issuer, cvm_code, total_shares, fetched_at)
             values ($1, $2, $3, $4)
             on conflict (issuer) do update set
                cvm_code = excluded.cvm_code, total_shares = excluded.total_shares,
                fetched_at = excluded.fetched_at, claimed_until = null",
        )
        .bind(issuer)
        .bind(info.cvm_code)
        .bind(info.total_shares)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn release_issuer_code(&self, issuer: &str) -> Result<(), StoreError> {
        sqlx::query("update b3_issuer_cvm set claimed_until = null where issuer = $1")
            .bind(issuer)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn cvm_source_states(&self) -> Result<HashMap<String, SourceState>, StoreError> {
        let rows: Vec<SourceRow> = sqlx::query_as(
            "select source, etag, last_modified, scanned_codes, attempted_at, checked_at, last_error
               from cvm_sources",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(source_state).collect())
    }

    /// Reserva a leitura de um arquivo e devolve o estado dele lido na mesma
    /// consulta que reservou. `None`: outra réplica está lendo.
    ///
    /// O estado volta junto porque a lista que a rodada leu no começo pode
    /// estar velha: outra réplica pode ter acabado de ler este arquivo e soltado
    /// a reserva. Quem reserva decide de novo com este estado, não com aquele.
    pub async fn claim_cvm_source(
        &self,
        key: &str,
        now: OffsetDateTime,
        lease: time::Duration,
    ) -> Result<Option<SourceState>, StoreError> {
        let claimed: Option<SourceRow> = sqlx::query_as(
            "insert into cvm_sources as s (source, claimed_until)
             values ($1, $2::timestamptz + $3::interval)
             on conflict (source) do update set claimed_until = excluded.claimed_until
              where s.claimed_until is null or s.claimed_until <= $2::timestamptz
             returning source, etag, last_modified, scanned_codes, attempted_at, checked_at,
                       last_error",
        )
        .bind(key)
        .bind(now)
        .bind(pg_interval(lease))
        .fetch_optional(&self.pool)
        .await?;
        Ok(claimed.map(|row| source_state(row).1))
    }

    /// Solta a reserva de um arquivo que, relido o estado, não precisava mais
    /// ser lido.
    pub async fn release_cvm_source(&self, key: &str) -> Result<(), StoreError> {
        sqlx::query("update cvm_sources set claimed_until = null where source = $1")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Uma leitura que falhou: os dados e a versão ficam como estavam.
    pub async fn record_cvm_source_failure(
        &self,
        key: &str,
        now: OffsetDateTime,
        error: &str,
    ) -> Result<(), StoreError> {
        let error: String = error.chars().take(300).collect();
        sqlx::query(
            "insert into cvm_sources (source, attempted_at, last_error) values ($1, $2, $3)
             on conflict (source) do update set
                attempted_at = excluded.attempted_at,
                last_error = excluded.last_error,
                claimed_until = null",
        )
        .bind(key)
        .bind(now)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 304: a versão gravada continua valendo.
    pub async fn record_cvm_source_unchanged(
        &self,
        key: &str,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "update cvm_sources set attempted_at = $2, checked_at = $2, last_error = null,
                    claimed_until = null
              where source = $1",
        )
        .bind(key)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// O arquivo do ano ainda não existe (404): nada a ler, até a próxima
    /// conferência.
    pub async fn record_cvm_source_absent(
        &self,
        key: &str,
        scanned: &BTreeSet<i32>,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, key).await?;
        mark_checked(&mut tx, key, &Validators::default(), scanned, now).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Grava os documentos lidos de um arquivo. Um documento só troca o
    /// gravado se a versão (`VERSAO`) for igual ou maior: reapresentação vence,
    /// arquivo velho não desfaz reapresentação.
    pub async fn save_company_filings(
        &self,
        key: &str,
        filings: &[Filing],
        validators: &Validators,
        scanned: &BTreeSet<i32>,
        now: OffsetDateTime,
    ) -> Result<usize, StoreError> {
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, key).await?;
        let mut written = 0;
        for filing in filings {
            let replaced: Option<(i32,)> = sqlx::query_as(
                "insert into cvm_filings as f
                    (cvm_code, kind, reference_date, version, scope, company_name, source, fetched_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8)
                 on conflict (cvm_code, kind, reference_date) do update set
                    version = excluded.version, scope = excluded.scope,
                    company_name = excluded.company_name, source = excluded.source,
                    fetched_at = excluded.fetched_at
                  where f.version <= excluded.version
                 returning version",
            )
            .bind(filing.cvm_code)
            .bind(filing.kind.as_str())
            .bind(filing.reference_date)
            .bind(filing.version)
            .bind(filing.scope.as_str())
            .bind(&filing.company_name)
            .bind(key)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await?;
            if replaced.is_none() {
                continue;
            }
            written += 1;
            sqlx::query(
                "delete from cvm_filing_lines
                  where cvm_code = $1 and kind = $2 and reference_date = $3",
            )
            .bind(filing.cvm_code)
            .bind(filing.kind.as_str())
            .bind(filing.reference_date)
            .execute(&mut *tx)
            .await?;
            let lines = &filing.lines;
            let statements: Vec<&str> = lines.iter().map(|l| l.statement.as_str()).collect();
            let periods: Vec<&str> = lines.iter().map(|l| l.column.as_str()).collect();
            let starts: Vec<Option<Date>> = lines.iter().map(|l| l.period_start).collect();
            let ends: Vec<Date> = lines.iter().map(|l| l.period_end).collect();
            let codes: Vec<&str> = lines.iter().map(|l| l.code.as_str()).collect();
            let descriptions: Vec<&str> = lines.iter().map(|l| l.description.as_str()).collect();
            let values: Vec<Decimal> = lines.iter().map(|l| l.value).collect();
            sqlx::query(
                "insert into cvm_filing_lines
                    (cvm_code, kind, reference_date, statement, period, period_start, period_end,
                     code, description, value)
                 select $1, $2, $3, statement, period, period_start, period_end, code, description, value
                   from unnest($4::text[], $5::text[], $6::date[], $7::date[], $8::text[], $9::text[],
                               $10::numeric[])
                        as l (statement, period, period_start, period_end, code, description, value)",
            )
            .bind(filing.cvm_code)
            .bind(filing.kind.as_str())
            .bind(filing.reference_date)
            .bind(&statements)
            .bind(&periods)
            .bind(&starts)
            .bind(&ends)
            .bind(&codes)
            .bind(&descriptions)
            .bind(&values)
            .execute(&mut *tx)
            .await?;
        }
        mark_checked(&mut tx, key, validators, scanned, now).await?;
        tx.commit().await?;
        Ok(written)
    }

    /// Grava o cadastro inteiro (atualiza quem veio; quem sumiu do arquivo fica).
    pub async fn save_registry(
        &self,
        key: &str,
        registrations: &[Registration],
        validators: &Validators,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let codes: Vec<i32> = registrations.iter().map(|r| r.cvm_code).collect();
        let cnpjs: Vec<&str> = registrations.iter().map(|r| r.cnpj.as_str()).collect();
        let names: Vec<&str> = registrations.iter().map(|r| r.name.as_str()).collect();
        let statuses: Vec<&str> = registrations.iter().map(|r| r.status.as_str()).collect();
        let issuer: Vec<Option<&str>> = registrations
            .iter()
            .map(|r| r.issuer_status.as_deref())
            .collect();
        let sectors: Vec<Option<&str>> =
            registrations.iter().map(|r| r.sector.as_deref()).collect();
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, key).await?;
        sqlx::query(
            "insert into cvm_companies (cvm_code, cnpj, name, status, issuer_status, sector, updated_at)
             select code, cnpj, name, status, issuer_status, sector, $7
               from unnest($1::int[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[])
                    as r (code, cnpj, name, status, issuer_status, sector)
             on conflict (cvm_code) do update set
                cnpj = excluded.cnpj, name = excluded.name, status = excluded.status,
                issuer_status = excluded.issuer_status, sector = excluded.sector,
                updated_at = excluded.updated_at",
        )
        .bind(&codes)
        .bind(&cnpjs)
        .bind(&names)
        .bind(&statuses)
        .bind(&issuer)
        .bind(&sectors)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        mark_checked(&mut tx, key, validators, &BTreeSet::new(), now).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Grava o informe mensal dos fundos: só troca o gravado por um informe
    /// mais novo (ou reapresentado). Sem a série de meses; ver
    /// [`Store::save_fund_monthly`].
    pub async fn save_fund_reports(
        &self,
        key: &str,
        reports: &[FundReport],
        validators: &Validators,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        self.save_fund_monthly(key, reports, &[], validators, now)
            .await
    }

    /// Grava o informe mensal dos fundos e a série de meses, numa transação:
    /// o informe só troca o gravado por um mais novo (ou reapresentado); cada
    /// mês, pela versão igual ou maior.
    pub async fn save_fund_monthly(
        &self,
        key: &str,
        reports: &[FundReport],
        months: &[FundMonth],
        validators: &Validators,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let cnpjs: Vec<&str> = reports.iter().map(|r| r.cnpj.as_str()).collect();
        let issuers: Vec<&str> = reports.iter().map(|r| r.issuer.as_str()).collect();
        let isins: Vec<&str> = reports.iter().map(|r| r.isin.as_str()).collect();
        let names: Vec<&str> = reports.iter().map(|r| r.name.as_str()).collect();
        let dates: Vec<Date> = reports.iter().map(|r| r.reference_date).collect();
        let versions: Vec<i32> = reports.iter().map(|r| r.version).collect();
        let assets: Vec<Option<Decimal>> = reports.iter().map(|r| r.net_assets).collect();
        let shares: Vec<Option<Decimal>> = reports.iter().map(|r| r.shares).collect();
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, key).await?;
        sqlx::query(
            "insert into cvm_funds as f
                (cnpj, issuer, isin, name, reference_date, version, net_assets, shares, updated_at)
             select cnpj, issuer, isin, name, reference_date, version, net_assets, shares, $9
               from unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::date[], $6::int[],
                           $7::numeric[], $8::numeric[])
                    as r (cnpj, issuer, isin, name, reference_date, version, net_assets, shares)
             on conflict (cnpj) do update set
                issuer = excluded.issuer, isin = excluded.isin, name = excluded.name,
                reference_date = excluded.reference_date, version = excluded.version,
                net_assets = excluded.net_assets, shares = excluded.shares,
                updated_at = excluded.updated_at
              where (f.reference_date, f.version) <= (excluded.reference_date, excluded.version)",
        )
        .bind(&cnpjs)
        .bind(&issuers)
        .bind(&isins)
        .bind(&names)
        .bind(&dates)
        .bind(&versions)
        .bind(&assets)
        .bind(&shares)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        if !months.is_empty() {
            let cnpjs: Vec<&str> = months.iter().map(|m| m.cnpj.as_str()).collect();
            let dates: Vec<Date> = months.iter().map(|m| m.reference_date).collect();
            let versions: Vec<i32> = months.iter().map(|m| m.version).collect();
            let assets: Vec<Option<Decimal>> = months.iter().map(|m| m.net_assets).collect();
            let shares: Vec<Option<Decimal>> = months.iter().map(|m| m.shares).collect();
            let yields: Vec<Option<Decimal>> = months.iter().map(|m| m.dividend_yield).collect();
            // O mesmo mês pode vir em mais de uma versão no arquivo: a maior.
            sqlx::query(
                "insert into cvm_fund_months as m
                    (cnpj, reference_date, version, net_assets, shares, dividend_yield, updated_at)
                 select distinct on (cnpj, reference_date)
                        cnpj, reference_date, version, net_assets, shares, dividend_yield, $7
                   from unnest($1::text[], $2::date[], $3::int[], $4::numeric[], $5::numeric[],
                               $6::numeric[])
                        as r (cnpj, reference_date, version, net_assets, shares, dividend_yield)
                  order by cnpj, reference_date, version desc
                 on conflict (cnpj, reference_date) do update set
                    version = excluded.version, net_assets = excluded.net_assets,
                    shares = excluded.shares, dividend_yield = excluded.dividend_yield,
                    updated_at = excluded.updated_at
                  where m.version <= excluded.version",
            )
            .bind(&cnpjs)
            .bind(&dates)
            .bind(&versions)
            .bind(&assets)
            .bind(&shares)
            .bind(&yields)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        mark_checked(&mut tx, key, validators, &BTreeSet::new(), now).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Grava a vacância do informe trimestral (o mais novo vence).
    pub async fn save_fund_vacancy(
        &self,
        key: &str,
        rows: &[FundVacancy],
        validators: &Validators,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let cnpjs: Vec<&str> = rows.iter().map(|r| r.cnpj.as_str()).collect();
        let dates: Vec<Date> = rows.iter().map(|r| r.vacancy.reference_date).collect();
        let values: Vec<Option<Decimal>> = rows.iter().map(|r| r.vacancy.value.value()).collect();
        let gaps: Vec<Option<&str>> = rows
            .iter()
            .map(|r| r.vacancy.value.gap().map(Gap::as_str))
            .collect();
        let counts: Vec<i32> = rows
            .iter()
            .map(|r| i32::try_from(r.vacancy.properties).unwrap_or(i32::MAX))
            .collect();
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, key).await?;
        sqlx::query(
            "insert into cvm_fund_vacancy as v
                (cnpj, reference_date, version, vacancy, vacancy_gap, properties, updated_at)
             select cnpj, reference_date, 0, vacancy, vacancy_gap, properties, $6
               from unnest($1::text[], $2::date[], $3::numeric[], $4::text[], $5::int[])
                    as r (cnpj, reference_date, vacancy, vacancy_gap, properties)
             on conflict (cnpj) do update set
                reference_date = excluded.reference_date, vacancy = excluded.vacancy,
                vacancy_gap = excluded.vacancy_gap, properties = excluded.properties,
                updated_at = excluded.updated_at
              where v.reference_date <= excluded.reference_date",
        )
        .bind(&cnpjs)
        .bind(&dates)
        .bind(&values)
        .bind(&gaps)
        .bind(&counts)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        mark_checked(&mut tx, key, validators, &BTreeSet::new(), now).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn company_filings(
        &self,
        codes: &[i32],
    ) -> Result<HashMap<i32, Vec<Filing>>, StoreError> {
        let filings: Vec<FilingRow> = sqlx::query_as(
            "select cvm_code, kind, reference_date, version, scope, company_name
               from cvm_filings where cvm_code = any($1)",
        )
        .bind(codes)
        .fetch_all(&self.pool)
        .await?;
        let lines: Vec<LineRow> = sqlx::query_as(
            "select cvm_code, kind, reference_date, statement, period, period_start, period_end,
                    code, description, value
               from cvm_filing_lines where cvm_code = any($1)
              order by cvm_code, kind, reference_date, statement, period, code",
        )
        .bind(codes)
        .fetch_all(&self.pool)
        .await?;
        let mut by_filing: HashMap<(i32, String, Date), Vec<Line>> = HashMap::new();
        for row in lines {
            let line = Line {
                statement: Statement::parse(&row.statement)
                    .ok_or_else(|| decode_error(&row.statement))?,
                column: Column::parse(&row.period).ok_or_else(|| decode_error(&row.period))?,
                period_start: row.period_start,
                period_end: row.period_end,
                code: row.code,
                description: row.description,
                value: row.value.normalize(),
            };
            by_filing
                .entry((row.cvm_code, row.kind, row.reference_date))
                .or_default()
                .push(line);
        }
        let mut out: HashMap<i32, Vec<Filing>> = HashMap::new();
        for row in filings {
            let kind = FilingKind::parse(&row.kind).ok_or_else(|| decode_error(&row.kind))?;
            let scope = Scope::parse(&row.scope).ok_or_else(|| decode_error(&row.scope))?;
            let lines = by_filing
                .remove(&(row.cvm_code, row.kind, row.reference_date))
                .unwrap_or_default();
            out.entry(row.cvm_code).or_default().push(Filing {
                cvm_code: row.cvm_code,
                kind,
                reference_date: row.reference_date,
                version: row.version,
                scope,
                company_name: row.company_name,
                lines,
            });
        }
        Ok(out)
    }

    /// Os indicadores dos ativos da carteira da pessoa, por símbolo. ETF,
    /// BDR, ativo de fora da B3 e o que ainda não foi buscado vêm como
    /// [`AssetFundamentals::Unsupported`] com o motivo. Ativo sem símbolo
    /// (renda fixa por valor) não entra.
    pub async fn fundamentals_for_user(
        &self,
        user: Uuid,
    ) -> Result<HashMap<String, AssetFundamentals>, StoreError> {
        let assets = self.assets(user).await?;
        let assets: Vec<&Asset> = assets.iter().collect();
        self.fundamentals_for_assets(&assets).await
    }

    /// Os indicadores só dos ativos dados (o aviso do aporte lê só os do
    /// plano). Mesmo resultado de [`Store::fundamentals_for_user`] para eles.
    pub async fn fundamentals_for_assets(
        &self,
        assets: &[&Asset],
    ) -> Result<HashMap<String, AssetFundamentals>, StoreError> {
        let mut out = HashMap::new();
        let mut companies: Vec<(String, String)> = Vec::new();
        let mut funds: Vec<(String, String)> = Vec::new();
        for asset in assets.iter().copied() {
            let Some(symbol) = asset.symbol.clone() else {
                continue;
            };
            match coverage_of(
                asset.asset_class,
                asset.listed_on_b3,
                asset.treasury,
                Some(&symbol),
            ) {
                Coverage::Company { issuer } => companies.push((symbol, issuer)),
                Coverage::Fund { issuer } => funds.push((symbol, issuer)),
                Coverage::Unsupported(reason) => {
                    out.insert(symbol, AssetFundamentals::Unsupported(reason));
                }
            }
        }
        let sources = self.cvm_source_states().await?;
        let synced = |prefix: &str| {
            sources
                .iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .filter_map(|(_, state)| state.checked_at)
                .max()
        };
        if !companies.is_empty() {
            self.companies_into(&companies, &sources, synced("annual:"), &mut out)
                .await?;
        }
        if !funds.is_empty() {
            let fund_synced = synced("fund_monthly:");
            self.funds_into(&funds, fund_synced, &mut out).await?;
        }
        Ok(out)
    }

    async fn companies_into(
        &self,
        companies: &[(String, String)],
        sources: &HashMap<String, SourceState>,
        synced_at: Option<OffsetDateTime>,
        out: &mut HashMap<String, AssetFundamentals>,
    ) -> Result<(), StoreError> {
        let issuers: Vec<String> = companies.iter().map(|(_, i)| i.clone()).collect();
        let codes = self.issuer_codes(&issuers).await?;
        let wanted: Vec<i32> = codes.values().filter_map(|c| c.cvm_code).collect();
        let filings = self.company_filings(&wanted).await?;
        let registrations: Vec<RegistrationRow> = sqlx::query_as(
            "select cvm_code, cnpj, name, status, issuer_status, sector
               from cvm_companies where cvm_code = any($1)",
        )
        .bind(&wanted)
        .fetch_all(&self.pool)
        .await?;
        let registrations: HashMap<i32, Registration> = registrations
            .into_iter()
            .map(|r| {
                (
                    r.cvm_code,
                    Registration {
                        cvm_code: r.cvm_code,
                        cnpj: r.cnpj,
                        name: r.name,
                        status: r.status,
                        issuer_status: r.issuer_status,
                        sector: r.sector,
                    },
                )
            })
            .collect();
        // Códigos que algum arquivo anual já leu: sem documento depois disso
        // é "sem DFP", não "ainda buscando".
        let scanned: HashSet<i32> = sources
            .iter()
            .filter(|(key, state)| key.starts_with("annual:") && state.checked_at.is_some())
            .flat_map(|(_, state)| state.scanned.iter().copied())
            .collect();
        for (symbol, issuer) in companies {
            let entry = match codes.get(issuer) {
                None
                | Some(IssuerCode {
                    fetched_at: None, ..
                }) => AssetFundamentals::Unsupported(Unsupported::Pending),
                Some(IssuerCode { cvm_code: None, .. }) => {
                    AssetFundamentals::Unsupported(Unsupported::NoCvmCode)
                }
                Some(IssuerCode {
                    cvm_code: Some(code),
                    total_shares,
                    ..
                }) => {
                    let docs = filings.get(code).map(Vec::as_slice).unwrap_or_default();
                    match company_metrics(
                        *code,
                        docs,
                        registrations.get(code),
                        *total_shares,
                        synced_at,
                    ) {
                        Some(metrics) => AssetFundamentals::Company(Box::new(metrics)),
                        None if scanned.contains(code) => {
                            AssetFundamentals::Unsupported(Unsupported::NoFilings)
                        }
                        None => AssetFundamentals::Unsupported(Unsupported::Pending),
                    }
                }
            };
            out.insert(symbol.clone(), entry);
        }
        Ok(())
    }

    async fn funds_into(
        &self,
        funds: &[(String, String)],
        synced_at: Option<OffsetDateTime>,
        out: &mut HashMap<String, AssetFundamentals>,
    ) -> Result<(), StoreError> {
        let issuers: Vec<String> = funds.iter().map(|(_, i)| i.clone()).collect();
        // Dois CNPJs com o mesmo emissor (fundo que virou classe): o informe
        // mais novo.
        let rows: Vec<FundRow> = sqlx::query_as(
            "select distinct on (f.issuer)
                    f.cnpj, f.issuer, f.isin, f.name, f.reference_date, f.version, f.net_assets,
                    f.shares, v.reference_date as vacancy_reference_date, v.vacancy, v.vacancy_gap,
                    v.properties
               from cvm_funds f
               left join cvm_fund_vacancy v on v.cnpj = f.cnpj
              where f.issuer = any($1)
              order by f.issuer, f.reference_date desc, f.version desc, f.cnpj",
        )
        .bind(&issuers)
        .fetch_all(&self.pool)
        .await?;
        let symbols: Vec<String> = funds
            .iter()
            .filter_map(|(symbol, _)| Market::B3.quote_symbol(symbol))
            .collect();
        let quotes = self.quotes(&symbols).await?;
        let mut by_issuer: HashMap<String, (FundReport, Option<Vacancy>)> = HashMap::new();
        for row in rows {
            let vacancy = match (
                row.vacancy_reference_date,
                row.vacancy,
                row.vacancy_gap.as_deref(),
            ) {
                (Some(reference_date), Some(v), _) => Some(Vacancy {
                    reference_date,
                    value: Figure::known(v.normalize()),
                    properties: u32::try_from(row.properties.unwrap_or(0)).unwrap_or(0),
                }),
                (Some(reference_date), None, Some(gap)) => Some(Vacancy {
                    reference_date,
                    value: Figure::missing(Gap::parse(gap).ok_or_else(|| decode_error(gap))?),
                    properties: 0,
                }),
                _ => None,
            };
            let report = FundReport {
                cnpj: row.cnpj,
                issuer: row.issuer.clone(),
                isin: row.isin,
                name: row.name,
                reference_date: row.reference_date,
                version: row.version,
                net_assets: row.net_assets.map(|d| d.normalize()),
                shares: row.shares.map(|d| d.normalize()),
            };
            by_issuer.insert(row.issuer, (report, vacancy));
        }
        for (symbol, issuer) in funds {
            let entry = match by_issuer.get(issuer) {
                Some((report, vacancy)) => {
                    let price = Market::B3
                        .quote_symbol(symbol)
                        .and_then(|s| quotes.get(&s))
                        .map(|q| q.price);
                    AssetFundamentals::Fund(Box::new(fund_metrics(
                        report,
                        vacancy.as_ref(),
                        price,
                        synced_at,
                    )))
                }
                None if synced_at.is_some() => {
                    AssetFundamentals::Unsupported(Unsupported::FundNotFound)
                }
                None => AssetFundamentals::Unsupported(Unsupported::Pending),
            };
            out.insert(symbol.clone(), entry);
        }
        Ok(())
    }
}
