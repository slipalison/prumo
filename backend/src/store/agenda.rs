//! Agenda no Postgres (migração 0028): os documentos entregues pelas
//! companhias (CVM IPE) e pelos FIIs (Fundos.NET) da carteira, a data de
//! entrega de cada ITR e DFP e a vez de cada réplica buscar um fundo.
//!
//! Tudo é referência compartilhada entre as carteiras, como os fundamentos: o
//! fato relevante de uma companhia é o mesmo para todo mundo. Toda escrita é
//! uma transação curta com `pg_advisory_xact_lock`; nenhuma conexão fica presa
//! durante o download. O estado de cada arquivo da CVM mora em `cvm_sources`,
//! com as chaves da Agenda (`documents:2026`, `receipts_quarterly:2026`).

use std::collections::{BTreeSet, HashMap};

use sqlx::FromRow;
use time::{Date, OffsetDateTime};

use super::fundamentals::{lock, mark_checked};
use super::{decode_error, Store, StoreError};
use crate::agenda::cvm::{CompanyDocument, FilingReceipt};
use crate::agenda::fundos_net::FundDocument;
use crate::domain::agenda::{Document, DocumentCategory, DocumentSource};
use crate::domain::fundamentals::FilingKind;
use crate::fundamentals::download::Validators;

/// De quem é um documento.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DocumentOwner {
    /// A companhia, pelo código CVM.
    Company(i32),
    /// O fundo, pelo CNPJ como o informe da CVM escreve.
    Fund(String),
}

/// Quando as fontes da Agenda foram conferidas pela última vez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgendaFreshness {
    /// A conferência mais recente de um arquivo IPE da CVM.
    pub cvm_documents: Option<OffsetDateTime>,
    /// A busca mais velha entre os fundos pedidos no Fundos.NET.
    pub fund_documents: Option<OffsetDateTime>,
}

#[derive(FromRow)]
struct DocumentRow {
    source: String,
    cvm_code: Option<i32>,
    fund_cnpj: Option<String>,
    category: String,
    title: Option<String>,
    subject: Option<String>,
    reference_date: Option<Date>,
    delivered_on: Date,
    version: i32,
    url: String,
}

#[derive(FromRow)]
struct ReceiptRow {
    cvm_code: i32,
    kind: String,
    reference_date: Date,
    version: i32,
    received_on: Date,
    url: Option<String>,
}

/// A versão de um arquivo da CVM que acabou de ser lida.
#[derive(Debug, Clone, Copy)]
pub struct FileVersion<'a> {
    /// A chave em `cvm_sources`.
    pub key: &'a str,
    pub validators: &'a Validators,
    /// Os códigos CVM que esta versão já teve lidos.
    pub scanned: &'a BTreeSet<i32>,
    pub now: OffsetDateTime,
}

fn pg_interval(duration: time::Duration) -> String {
    format!("{} seconds", duration.whole_seconds())
}

/// Os documentos de uma lista, em colunas, para um `insert ... unnest`.
struct Columns<'a> {
    ids: Vec<&'a str>,
    categories: Vec<&'static str>,
    titles: Vec<Option<&'a str>>,
    subjects: Vec<Option<&'a str>>,
    references: Vec<Option<Date>>,
    delivered: Vec<Date>,
    versions: Vec<i32>,
    urls: Vec<&'a str>,
}

impl<'a> Columns<'a> {
    fn of(list: impl Iterator<Item = (&'a str, &'a Document)>) -> Self {
        let mut columns = Columns {
            ids: Vec::new(),
            categories: Vec::new(),
            titles: Vec::new(),
            subjects: Vec::new(),
            references: Vec::new(),
            delivered: Vec::new(),
            versions: Vec::new(),
            urls: Vec::new(),
        };
        for (id, document) in list {
            // Sem link não há documento original para abrir: não entra.
            let Some(url) = document.url.as_deref() else {
                continue;
            };
            columns.ids.push(id);
            columns.categories.push(document.category.as_str());
            columns.titles.push(document.title.as_deref());
            columns.subjects.push(document.subject.as_deref());
            columns.references.push(document.reference_date);
            columns.delivered.push(document.delivered_on);
            columns.versions.push(document.version);
            columns.urls.push(url);
        }
        columns
    }
}

impl Store {
    /// Grava os documentos lidos de um arquivo IPE (`year`, pela data de
    /// entrega) para as companhias `read`: o que a CVM tirou do arquivo sai
    /// daqui também. Marca o arquivo como lido com esta versão.
    pub async fn save_company_documents(
        &self,
        file: FileVersion<'_>,
        year: i32,
        documents: &[CompanyDocument],
        read: &BTreeSet<i32>,
    ) -> Result<usize, StoreError> {
        let codes: Vec<i32> = read.iter().copied().collect();
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, file.key).await?;
        sqlx::query(
            "delete from corporate_documents
              where source = 'cvm_ipe' and cvm_code = any($1)
                and delivered_on >= make_date($2, 1, 1) and delivered_on < make_date($2 + 1, 1, 1)",
        )
        .bind(&codes)
        .bind(year)
        .execute(&mut *tx)
        .await?;
        let columns = Columns::of(
            documents
                .iter()
                .map(|d| (d.document_id.as_str(), &d.document)),
        );
        let owners: Vec<i32> = documents
            .iter()
            .filter(|d| d.document.url.is_some())
            .map(|d| d.cvm_code)
            .collect();
        let written = sqlx::query(
            "insert into corporate_documents
                (source, document_id, cvm_code, category, title, subject, reference_date,
                 delivered_on, version, url, updated_at)
             select 'cvm_ipe', id, code, category, title, subject, reference, delivered, version,
                    url, $10
               from unnest($1::text[], $2::int[], $3::text[], $4::text[], $5::text[], $6::date[],
                           $7::date[], $8::int[], $9::text[])
                    as d (id, code, category, title, subject, reference, delivered, version, url)
             on conflict (source, document_id) do update set
                cvm_code = excluded.cvm_code, category = excluded.category,
                title = excluded.title, subject = excluded.subject,
                reference_date = excluded.reference_date, delivered_on = excluded.delivered_on,
                version = excluded.version, url = excluded.url, updated_at = excluded.updated_at",
        )
        .bind(&columns.ids)
        .bind(&owners)
        .bind(&columns.categories)
        .bind(&columns.titles)
        .bind(&columns.subjects)
        .bind(&columns.references)
        .bind(&columns.delivered)
        .bind(&columns.versions)
        .bind(&columns.urls)
        .bind(file.now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        mark_checked(&mut tx, file.key, file.validators, file.scanned, file.now).await?;
        tx.commit().await?;
        Ok(usize::try_from(written).unwrap_or(usize::MAX))
    }

    /// Grava a entrega de cada ITR ou DFP lida do índice. Uma versão menor
    /// nunca troca a maior.
    pub async fn save_filing_receipts(
        &self,
        file: FileVersion<'_>,
        receipts: &[FilingReceipt],
    ) -> Result<usize, StoreError> {
        let codes: Vec<i32> = receipts.iter().map(|r| r.cvm_code).collect();
        let kinds: Vec<&str> = receipts.iter().map(|r| r.kind.as_str()).collect();
        let references: Vec<Date> = receipts.iter().map(|r| r.reference_date).collect();
        let versions: Vec<i32> = receipts.iter().map(|r| r.version).collect();
        let received: Vec<Date> = receipts.iter().map(|r| r.received_on).collect();
        let urls: Vec<Option<&str>> = receipts.iter().map(|r| r.url.as_deref()).collect();
        let mut tx = self.pool.begin().await?;
        lock(&mut tx, file.key).await?;
        let written = sqlx::query(
            "insert into cvm_filing_receipts as r
                (cvm_code, kind, reference_date, version, received_on, url, updated_at)
             select code, kind, reference, version, received, url, $7
               from unnest($1::int[], $2::text[], $3::date[], $4::int[], $5::date[], $6::text[])
                    as d (code, kind, reference, version, received, url)
             on conflict (cvm_code, kind, reference_date) do update set
                version = excluded.version, received_on = excluded.received_on,
                url = excluded.url, updated_at = excluded.updated_at
              where r.version <= excluded.version",
        )
        .bind(&codes)
        .bind(&kinds)
        .bind(&references)
        .bind(&versions)
        .bind(&received)
        .bind(&urls)
        .bind(file.now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        mark_checked(&mut tx, file.key, file.validators, file.scanned, file.now).await?;
        tx.commit().await?;
        Ok(usize::try_from(written).unwrap_or(usize::MAX))
    }

    /// Apaga os documentos entregues antes de `before` (a Agenda não mostra
    /// mais que isso para trás).
    pub async fn prune_corporate_documents(&self, before: Date) -> Result<u64, StoreError> {
        let done = sqlx::query("delete from corporate_documents where delivered_on < $1")
            .bind(before)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected())
    }

    /// A vez de buscar os documentos de um fundo: `true` para quem marcou
    /// primeiro, e só se a última busca venceu (`ttl`), a última falha já
    /// esperou `retry` e ninguém está buscando (`lease`).
    pub async fn claim_fund_documents(
        &self,
        cnpj: &str,
        now: OffsetDateTime,
        ttl: time::Duration,
        retry: time::Duration,
        lease: time::Duration,
    ) -> Result<bool, StoreError> {
        let claimed: Option<(String,)> = sqlx::query_as(
            "insert into fund_document_fetches as f (cnpj, claimed_until)
             values ($1, $2::timestamptz + $5::interval)
             on conflict (cnpj) do update set claimed_until = excluded.claimed_until
              where (f.fetched_at is null or f.fetched_at <= $2::timestamptz - $3::interval)
                and (f.last_error is null or f.attempted_at is null
                     or f.attempted_at <= $2::timestamptz - $4::interval)
                and (f.claimed_until is null or f.claimed_until <= $2::timestamptz)
             returning cnpj",
        )
        .bind(cnpj)
        .bind(now)
        .bind(pg_interval(ttl))
        .bind(pg_interval(retry))
        .bind(pg_interval(lease))
        .fetch_optional(&self.pool)
        .await?;
        Ok(claimed.is_some())
    }

    /// Grava os documentos de um fundo. A busca traz os mais novos: o que o
    /// Fundos.NET deixou de listar dentro do período que a resposta cobre (foi
    /// substituído ou retirado) sai; o mais velho que isso fica.
    pub async fn save_fund_documents(
        &self,
        cnpj: &str,
        documents: &[FundDocument],
        now: OffsetDateTime,
    ) -> Result<usize, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(format!("fund_documents:{cnpj}"))
            .execute(&mut *tx)
            .await?;
        if let Some(oldest) = documents.iter().map(|d| d.document.delivered_on).min() {
            let ids: Vec<&str> = documents.iter().map(|d| d.document_id.as_str()).collect();
            sqlx::query(
                "delete from corporate_documents
                  where source = 'fundos_net' and fund_cnpj = $1 and delivered_on >= $2
                    and not (document_id = any($3))",
            )
            .bind(cnpj)
            .bind(oldest)
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        }
        let columns = Columns::of(
            documents
                .iter()
                .map(|d| (d.document_id.as_str(), &d.document)),
        );
        let written = sqlx::query(
            "insert into corporate_documents
                (source, document_id, fund_cnpj, category, title, subject, reference_date,
                 delivered_on, version, url, updated_at)
             select 'fundos_net', id, $1, category, title, subject, reference, delivered, version,
                    url, $10
               from unnest($2::text[], $3::text[], $4::text[], $5::text[], $6::date[], $7::date[],
                           $8::int[], $9::text[])
                    as d (id, category, title, subject, reference, delivered, version, url)
             on conflict (source, document_id) do update set
                fund_cnpj = excluded.fund_cnpj, category = excluded.category,
                title = excluded.title, subject = excluded.subject,
                reference_date = excluded.reference_date, delivered_on = excluded.delivered_on,
                version = excluded.version, url = excluded.url, updated_at = excluded.updated_at",
        )
        .bind(cnpj)
        .bind(&columns.ids)
        .bind(&columns.categories)
        .bind(&columns.titles)
        .bind(&columns.subjects)
        .bind(&columns.references)
        .bind(&columns.delivered)
        .bind(&columns.versions)
        .bind(&columns.urls)
        .bind(now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query(
            "update fund_document_fetches set fetched_at = $2, attempted_at = $2,
                    last_error = null, claimed_until = null
              where cnpj = $1",
        )
        .bind(cnpj)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(usize::try_from(written).unwrap_or(usize::MAX))
    }

    /// Uma busca de fundo que falhou: o gravado fica, e a próxima tentativa
    /// espera.
    pub async fn record_fund_documents_failure(
        &self,
        cnpj: &str,
        now: OffsetDateTime,
        error: &str,
    ) -> Result<(), StoreError> {
        let error: String = error.chars().take(300).collect();
        sqlx::query(
            "insert into fund_document_fetches (cnpj, attempted_at, last_error) values ($1, $2, $3)
             on conflict (cnpj) do update set
                attempted_at = excluded.attempted_at, last_error = excluded.last_error,
                claimed_until = null",
        )
        .bind(cnpj)
        .bind(now)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// O CNPJ de cada FII pelo emissor, para o Fundos.NET: o caminho único
    /// de ticker de FII → fundo (`Store::fund_cnpjs`).
    pub async fn agenda_fund_cnpjs(
        &self,
        issuers: &[String],
    ) -> Result<HashMap<String, String>, StoreError> {
        Ok(self
            .fund_cnpjs(issuers)
            .await?
            .into_iter()
            .map(|(issuer, choice)| (issuer, choice.cnpj))
            .collect())
    }

    /// Os documentos das companhias `codes` e dos fundos `cnpjs` entregues em
    /// `[from, to]`.
    pub async fn agenda_documents(
        &self,
        codes: &[i32],
        cnpjs: &[String],
        from: Date,
        to: Date,
    ) -> Result<Vec<(DocumentOwner, Document)>, StoreError> {
        if codes.is_empty() && cnpjs.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<DocumentRow> = sqlx::query_as(
            "select source, cvm_code, fund_cnpj, category, title, subject, reference_date,
                    delivered_on, version, url
               from corporate_documents
              where (cvm_code = any($1) or fund_cnpj = any($2))
                and delivered_on between $3 and $4
              order by delivered_on, source, document_id",
        )
        .bind(codes)
        .bind(cnpjs)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let source =
                    DocumentSource::parse(&r.source).ok_or_else(|| decode_error(&r.source))?;
                let category = DocumentCategory::parse(&r.category)
                    .ok_or_else(|| decode_error(&r.category))?;
                let owner = match (r.cvm_code, r.fund_cnpj) {
                    (Some(code), _) => DocumentOwner::Company(code),
                    (None, Some(cnpj)) => DocumentOwner::Fund(cnpj),
                    (None, None) => return Err(decode_error("documento sem dono")),
                };
                Ok((
                    owner,
                    Document {
                        source,
                        category,
                        title: r.title,
                        subject: r.subject,
                        reference_date: r.reference_date,
                        delivered_on: r.delivered_on,
                        version: r.version,
                        url: Some(r.url),
                        filing: None,
                    },
                ))
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(StoreError::from)
    }

    /// A entrega dos ITR e DFP das companhias `codes` recebidos em `[from, to]`.
    pub async fn filing_receipts(
        &self,
        codes: &[i32],
        from: Date,
        to: Date,
    ) -> Result<Vec<FilingReceipt>, StoreError> {
        if codes.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<ReceiptRow> = sqlx::query_as(
            "select cvm_code, kind, reference_date, version, received_on, url
               from cvm_filing_receipts
              where cvm_code = any($1) and received_on between $2 and $3
              order by received_on, cvm_code, kind, reference_date",
        )
        .bind(codes)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(FilingReceipt {
                    cvm_code: r.cvm_code,
                    kind: FilingKind::parse(&r.kind).ok_or_else(|| decode_error(&r.kind))?,
                    reference_date: r.reference_date,
                    version: r.version,
                    received_on: r.received_on,
                    url: r.url,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(StoreError::from)
    }

    /// Quando as fontes da Agenda foram conferidas (`cnpjs`: os fundos da
    /// pessoa).
    pub async fn agenda_freshness(&self, cnpjs: &[String]) -> Result<AgendaFreshness, StoreError> {
        let (cvm_documents,): (Option<OffsetDateTime>,) = sqlx::query_as(
            "select max(checked_at) from cvm_sources where source like 'documents:%'",
        )
        .fetch_one(&self.pool)
        .await?;
        let (fund_documents,): (Option<OffsetDateTime>,) = sqlx::query_as(
            "select min(fetched_at) from fund_document_fetches where cnpj = any($1)",
        )
        .bind(cnpjs)
        .fetch_one(&self.pool)
        .await?;
        Ok(AgendaFreshness {
            cvm_documents,
            fund_documents,
        })
    }
}
