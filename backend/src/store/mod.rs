//! Acesso ao Postgres. Toda consulta filtra pelo dono: um id de outra pessoa
//! se comporta como id inexistente.
//!
//! Cada área nova mora num submódulo (`store/<area>.rs`, com o próprio
//! `impl Store`): o submódulo enxerga o `pool` e este arquivo não cresce.

use std::collections::{BTreeSet, HashMap};

use rust_decimal::Decimal;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{FromRow, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::allocation::{round_cents, Targets};
use crate::domain::asset_class::{AssetClass, Checklist, Market, UnitKind};
use crate::domain::defaults;
use crate::live::Tick;
use crate::quotes::Quote;

mod treasury;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("registro duplicado")]
    Duplicate,
    #[error("registro inexistente")]
    NotFound,
    #[error("dado invalido: {0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub id: Uuid,
    pub asset_class: AssetClass,
    pub symbol: Option<String>,
    pub display_name: String,
    pub quantity: Decimal,
    pub manual_price: Option<Decimal>,
    pub score_override: Option<u8>,
    /// Negociado na B3 (ETF, BDR) mesmo numa classe de fora dela. Sempre
    /// verdadeiro em Ações Nacionais e FIIs.
    pub listed_on_b3: bool,
    /// Título do Tesouro Direto: o símbolo é a chave do título e o preço vem da
    /// tabela diária do Tesouro. Só em Renda Fixa.
    pub treasury: bool,
}

impl Asset {
    pub fn market(&self) -> Market {
        self.asset_class.market_of(self.listed_on_b3, self.treasury)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAsset {
    pub asset_class: AssetClass,
    pub symbol: Option<String>,
    pub display_name: String,
    pub quantity: Decimal,
    pub manual_price: Option<Decimal>,
    pub score_override: Option<u8>,
    pub listed_on_b3: bool,
    pub treasury: bool,
}

/// Campos ausentes ficam como estão; `Some(None)` limpa.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssetPatch {
    pub listed_on_b3: Option<bool>,
    pub quantity: Option<Decimal>,
    pub display_name: Option<String>,
    pub manual_price: Option<Option<Decimal>>,
    pub score_override: Option<Option<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub id: Uuid,
    pub checklist: Checklist,
    pub position: i32,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub asset_id: Uuid,
    pub question_id: Uuid,
    pub answer: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredQuote {
    pub symbol: String,
    pub price: Decimal,
    pub currency: String,
    pub name: Option<String>,
    pub fetched_at: OffsetDateTime,
    /// Hora do negócio que formou o preço, quando o provedor diz.
    pub traded_at: Option<OffsetDateTime>,
}

impl StoredQuote {
    /// De quando é o preço: a hora do negócio ou, sem ela, a da busca.
    pub fn as_of(&self) -> OffsetDateTime {
        self.traded_at.unwrap_or(self.fetched_at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewContributionItem {
    pub asset_id: Uuid,
    pub quantity: Decimal,
    pub unit_price: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContributionItem {
    pub asset_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub display_name: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub value: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
    pub id: Uuid,
    pub created_at: OffsetDateTime,
    pub amount: Decimal,
    pub items: Vec<ContributionItem>,
}

#[derive(FromRow)]
struct AssetRow {
    id: Uuid,
    asset_class: String,
    symbol: Option<String>,
    display_name: String,
    quantity: Decimal,
    manual_price: Option<Decimal>,
    score_override: Option<i16>,
    listed_on_b3: bool,
    treasury: bool,
}

#[derive(FromRow)]
struct QuestionRow {
    id: Uuid,
    checklist: String,
    position: i32,
    prompt: String,
}

#[derive(FromRow)]
struct QuoteRow {
    symbol: String,
    price: Decimal,
    currency: String,
    name: Option<String>,
    fetched_at: OffsetDateTime,
    traded_at: Option<OffsetDateTime>,
}

#[derive(FromRow)]
struct ContributionRow {
    id: Uuid,
    created_at: OffsetDateTime,
    amount: Decimal,
}

#[derive(FromRow)]
struct ContributionItemRow {
    contribution_id: Uuid,
    asset_id: Option<Uuid>,
    symbol: Option<String>,
    display_name: String,
    quantity: Decimal,
    unit_price: Decimal,
    value: Decimal,
}

/// As colunas de `assets`, como macro para entrar em `concat!`: o sqlx 0.9 só
/// aceita SQL estático (`&'static str`), e a lista é a única parte repetida.
macro_rules! asset_columns {
    () => {
        "id, asset_class, symbol, display_name, quantity, manual_price, score_override, listed_on_b3, treasury"
    };
}

fn decode_error(what: &str) -> sqlx::Error {
    sqlx::Error::Decode(format!("valor desconhecido no banco: {what}").into())
}

impl TryFrom<AssetRow> for Asset {
    type Error = sqlx::Error;

    fn try_from(row: AssetRow) -> Result<Self, Self::Error> {
        Ok(Asset {
            id: row.id,
            asset_class: AssetClass::parse(&row.asset_class)
                .ok_or_else(|| decode_error(&row.asset_class))?,
            symbol: row.symbol,
            display_name: row.display_name,
            quantity: row.quantity.normalize(),
            manual_price: row.manual_price.map(|p| p.normalize()),
            score_override: row.score_override.and_then(|s| u8::try_from(s).ok()),
            listed_on_b3: row.listed_on_b3,
            treasury: row.treasury,
        })
    }
}

impl TryFrom<QuestionRow> for Question {
    type Error = sqlx::Error;

    fn try_from(row: QuestionRow) -> Result<Self, Self::Error> {
        Ok(Question {
            id: row.id,
            checklist: Checklist::parse(&row.checklist)
                .ok_or_else(|| decode_error(&row.checklist))?,
            position: row.position,
            prompt: row.prompt,
        })
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

/// O que um ativo vale em reais: ativos por valor guardam o próprio valor em
/// `quantity`; os outros, quantidade × preço.
pub fn unit_value(unit: UnitKind, quantity: Decimal, unit_price: Decimal) -> Decimal {
    match unit {
        UnitKind::Value => quantity,
        UnitKind::Whole | UnitKind::Fractional | UnitKind::Hundredth => quantity * unit_price,
    }
}

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

impl Store {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Aplica as migrations. Duas réplicas subindo juntas não brigam: o sqlx
    /// segura um advisory lock enquanto migra.
    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        MIGRATOR.run(&self.pool).await
    }

    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        sqlx::query("select 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    /// O id da pessoa, criando a carteira (com as perguntas padrão) no primeiro acesso.
    pub async fn ensure_user(&self, email: &str) -> Result<Uuid, StoreError> {
        let mut tx = self.pool.begin().await?;
        let inserted: Option<(Uuid,)> =
            sqlx::query_as("insert into users (id, email) values ($1, $2) on conflict (email) do nothing returning id")
                .bind(Uuid::new_v4())
                .bind(email)
                .fetch_optional(&mut *tx)
                .await?;
        let id = match inserted {
            Some((id,)) => {
                for checklist in Checklist::ALL {
                    seed_questions(&mut tx, id, checklist).await?;
                }
                id
            }
            None => {
                let (id,): (Uuid,) = sqlx::query_as("select id from users where email = $1")
                    .bind(email)
                    .fetch_one(&mut *tx)
                    .await?;
                id
            }
        };
        tx.commit().await?;
        Ok(id)
    }

    pub async fn goals(&self, user: Uuid) -> Result<Targets, StoreError> {
        let rows: Vec<(String, i32)> =
            sqlx::query_as("select asset_class, target_bp from goals where user_id = $1")
                .bind(user)
                .fetch_all(&self.pool)
                .await?;
        let mut targets = Targets::default();
        for (class, bp) in rows {
            let class = AssetClass::parse(&class).ok_or_else(|| decode_error(&class))?;
            targets.set(class, u32::try_from(bp).unwrap_or(0));
        }
        Ok(targets)
    }

    pub async fn set_goals(&self, user: Uuid, targets: Targets) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        for class in AssetClass::ALL {
            sqlx::query(
                "insert into goals (user_id, asset_class, target_bp) values ($1, $2, $3)
                 on conflict (user_id, asset_class) do update set target_bp = excluded.target_bp",
            )
            .bind(user)
            .bind(class.as_str())
            .bind(i32::try_from(targets.get(class)).unwrap_or(0))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn assets(&self, user: Uuid) -> Result<Vec<Asset>, StoreError> {
        let rows: Vec<AssetRow> = sqlx::query_as(concat!(
            "select ",
            asset_columns!(),
            " from assets where user_id = $1 order by created_at, id"
        ))
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(Asset::try_from)
            .collect::<Result<_, _>>()?)
    }

    pub async fn asset(&self, user: Uuid, id: Uuid) -> Result<Option<Asset>, StoreError> {
        let row: Option<AssetRow> = sqlx::query_as(concat!(
            "select ",
            asset_columns!(),
            " from assets where user_id = $1 and id = $2"
        ))
        .bind(user)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Asset::try_from).transpose()?)
    }

    pub async fn insert_asset(&self, user: Uuid, new: &NewAsset) -> Result<Asset, StoreError> {
        let result: Result<AssetRow, sqlx::Error> = sqlx::query_as(concat!("insert into assets (id, user_id, asset_class, symbol, display_name, quantity, manual_price, score_override, listed_on_b3, treasury)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) returning ", asset_columns!(), ""))
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(new.asset_class.as_str())
        .bind(&new.symbol)
        .bind(&new.display_name)
        .bind(new.quantity)
        .bind(new.manual_price)
        .bind(new.score_override.map(i16::from))
        .bind(new.listed_on_b3)
        .bind(new.treasury)
        .fetch_one(&self.pool)
        .await;
        match result {
            Ok(row) => Ok(Asset::try_from(row)?),
            Err(error) if is_unique_violation(&error) => Err(StoreError::Duplicate),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn update_asset(
        &self,
        user: Uuid,
        id: Uuid,
        patch: &AssetPatch,
    ) -> Result<Option<Asset>, StoreError> {
        let row: Option<AssetRow> = sqlx::query_as(concat!(
            "update assets set
                quantity = coalesce($3, quantity),
                display_name = coalesce($4, display_name),
                manual_price = case when $5 then $6 else manual_price end,
                score_override = case when $7 then $8 else score_override end,
                listed_on_b3 = coalesce($9, listed_on_b3),
                updated_at = now()
             where user_id = $1 and id = $2
             returning ",
            asset_columns!(),
            ""
        ))
        .bind(user)
        .bind(id)
        .bind(patch.quantity)
        .bind(&patch.display_name)
        .bind(patch.manual_price.is_some())
        .bind(patch.manual_price.flatten())
        .bind(patch.score_override.is_some())
        .bind(patch.score_override.flatten().map(i16::from))
        .bind(patch.listed_on_b3)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Asset::try_from).transpose()?)
    }

    pub async fn delete_asset(&self, user: Uuid, id: Uuid) -> Result<bool, StoreError> {
        let done = sqlx::query("delete from assets where user_id = $1 and id = $2")
            .bind(user)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn answers(&self, user: Uuid) -> Result<Vec<Answer>, StoreError> {
        let rows: Vec<(Uuid, Uuid, bool)> = sqlx::query_as(
            "select a.asset_id, a.question_id, a.answer
               from asset_answers a join assets s on s.id = a.asset_id
              where s.user_id = $1",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(asset_id, question_id, answer)| Answer {
                asset_id,
                question_id,
                answer,
            })
            .collect())
    }

    /// Grava (`Some`) ou apaga (`None`) respostas. A pergunta tem de ser da
    /// pessoa e do checklist da classe do ativo.
    pub async fn set_answers(
        &self,
        user: Uuid,
        asset: &Asset,
        answers: &[(Uuid, Option<bool>)],
    ) -> Result<(), StoreError> {
        let Some(checklist) = asset.asset_class.checklist() else {
            return if answers.is_empty() {
                Ok(())
            } else {
                Err(StoreError::Invalid("essa classe nao tem checklist".into()))
            };
        };
        let mut tx = self.pool.begin().await?;
        for (question_id, answer) in answers {
            let owned: Option<(Uuid,)> =
                sqlx::query_as("select id from checklist_questions where id = $1 and user_id = $2 and checklist = $3")
                    .bind(question_id)
                    .bind(user)
                    .bind(checklist.as_str())
                    .fetch_optional(&mut *tx)
                    .await?;
            if owned.is_none() {
                return Err(StoreError::Invalid(format!(
                    "pergunta {question_id} nao pertence a este checklist"
                )));
            }
            match answer {
                Some(value) => {
                    sqlx::query(
                        "insert into asset_answers (asset_id, question_id, answer) values ($1, $2, $3)
                         on conflict (asset_id, question_id) do update set answer = excluded.answer",
                    )
                    .bind(asset.id)
                    .bind(question_id)
                    .bind(value)
                    .execute(&mut *tx)
                    .await?;
                }
                None => {
                    sqlx::query(
                        "delete from asset_answers where asset_id = $1 and question_id = $2",
                    )
                    .bind(asset.id)
                    .bind(question_id)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn questions(&self, user: Uuid) -> Result<Vec<Question>, StoreError> {
        let rows: Vec<QuestionRow> = sqlx::query_as(
            "select id, checklist, position, prompt from checklist_questions
              where user_id = $1 order by checklist, position, created_at",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(Question::try_from)
            .collect::<Result<_, _>>()?)
    }

    pub async fn add_question(
        &self,
        user: Uuid,
        checklist: Checklist,
        prompt: &str,
    ) -> Result<Question, StoreError> {
        let row: QuestionRow = sqlx::query_as(
            "insert into checklist_questions (id, user_id, checklist, position, prompt)
             values ($1, $2, $3,
                     (select coalesce(max(position), 0) + 1 from checklist_questions where user_id = $2 and checklist = $3),
                     $4)
             returning id, checklist, position, prompt",
        )
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(checklist.as_str())
        .bind(prompt)
        .fetch_one(&self.pool)
        .await?;
        Ok(Question::try_from(row)?)
    }

    pub async fn update_question(
        &self,
        user: Uuid,
        id: Uuid,
        prompt: &str,
    ) -> Result<Option<Question>, StoreError> {
        let row: Option<QuestionRow> = sqlx::query_as(
            "update checklist_questions set prompt = $3 where user_id = $1 and id = $2
             returning id, checklist, position, prompt",
        )
        .bind(user)
        .bind(id)
        .bind(prompt)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Question::try_from).transpose()?)
    }

    pub async fn delete_question(&self, user: Uuid, id: Uuid) -> Result<bool, StoreError> {
        let done = sqlx::query("delete from checklist_questions where user_id = $1 and id = $2")
            .bind(user)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Volta o checklist ao padrão: apaga as perguntas da pessoa (e as
    /// respostas delas, em cascata) e grava a lista padrão.
    pub async fn reset_checklist(
        &self,
        user: Uuid,
        checklist: Checklist,
    ) -> Result<Vec<Question>, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("delete from checklist_questions where user_id = $1 and checklist = $2")
            .bind(user)
            .bind(checklist.as_str())
            .execute(&mut *tx)
            .await?;
        seed_questions(&mut tx, user, checklist).await?;
        tx.commit().await?;
        Ok(self
            .questions(user)
            .await?
            .into_iter()
            .filter(|q| q.checklist == checklist)
            .collect())
    }

    pub async fn quotes(
        &self,
        symbols: &[String],
    ) -> Result<HashMap<String, StoredQuote>, StoreError> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<QuoteRow> = sqlx::query_as(
            "select symbol, price, currency, name, fetched_at, traded_at from quotes where symbol = any($1)",
        )
        .bind(symbols)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let quote = StoredQuote {
                    symbol: r.symbol.clone(),
                    price: r.price.normalize(),
                    currency: r.currency,
                    name: r.name,
                    fetched_at: r.fetched_at,
                    traded_at: r.traded_at,
                };
                (r.symbol, quote)
            })
            .collect())
    }

    /// Grava a cotação do `chart`. Moeda e nome sempre; o preço só se o
    /// negócio dele não for mais velho que o do preço gravado (o streamer pode
    /// ter trazido um mais novo).
    pub async fn upsert_quote(
        &self,
        quote: &Quote,
        fetched_at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into quotes (symbol, price, currency, name, fetched_at, traded_at)
             values ($1, $2, $3, $4, $5, $6)
             on conflict (symbol) do update set
                price = case when excluded.traded_at is null or quotes.traded_at is null
                                  or excluded.traded_at >= quotes.traded_at
                             then excluded.price else quotes.price end,
                traded_at = case when excluded.traded_at is null or quotes.traded_at is null
                                      or excluded.traded_at >= quotes.traded_at
                                 then excluded.traded_at else quotes.traded_at end,
                currency = excluded.currency,
                name = coalesce(excluded.name, quotes.name),
                fetched_at = excluded.fetched_at",
        )
        .bind(&quote.symbol)
        .bind(quote.price)
        .bind(&quote.currency)
        .bind(&quote.name)
        .bind(fetched_at)
        .bind(quote.traded_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Grava um lote de preços do streamer e devolve os símbolos cujo preço
    /// mudou. Só atualiza símbolo que já tem cotação (a moeda vem do `chart`)
    /// e só com negócio mais novo que o gravado. Um símbolo por lote.
    pub async fn apply_ticks(
        &self,
        ticks: &[Tick],
        fetched_at: OffsetDateTime,
    ) -> Result<BTreeSet<String>, StoreError> {
        if ticks.is_empty() {
            return Ok(BTreeSet::new());
        }
        let symbols: Vec<&str> = ticks.iter().map(|t| t.symbol.as_str()).collect();
        let prices: Vec<Decimal> = ticks.iter().map(|t| t.price).collect();
        let traded: Vec<OffsetDateTime> = ticks.iter().map(|t| t.traded_at).collect();
        // Todos os pedaços de um `with` leem a mesma foto do banco: `before`
        // enxerga o preço de antes do `update`.
        let moved: Vec<(String,)> = sqlx::query_as(
            "with batch as (
                 select * from unnest($1::text[], $2::numeric[], $3::timestamptz[])
                     as b (symbol, price, traded_at)
             ),
             before as (
                 select q.symbol, q.price from quotes q join batch b on b.symbol = q.symbol
             ),
             updated as (
                 update quotes q
                    set price = b.price, traded_at = b.traded_at, fetched_at = $4
                   from batch b
                  where q.symbol = b.symbol
                    and (q.traded_at is null or b.traded_at > q.traded_at)
                 returning q.symbol, q.price
             )
             select updated.symbol from updated
               join before on before.symbol = updated.symbol
              where updated.price <> before.price",
        )
        .bind(&symbols)
        .bind(&prices)
        .bind(&traded)
        .bind(fetched_at)
        .fetch_all(&self.pool)
        .await?;
        Ok(moved.into_iter().map(|(symbol,)| symbol).collect())
    }

    /// Mercado e símbolo de todos os ativos cotáveis, de todas as carteiras.
    pub async fn tracked_symbols(&self) -> Result<Vec<(Market, String)>, StoreError> {
        let rows: Vec<(String, bool, bool, String)> = sqlx::query_as(
            "select distinct asset_class, listed_on_b3, treasury, symbol from assets where symbol is not null",
        )
        .fetch_all(&self.pool)
        .await?;
        let tracked: BTreeSet<(Market, String)> = rows
            .into_iter()
            .filter_map(|(class, listed_on_b3, treasury, symbol)| {
                AssetClass::parse(&class).map(|c| (c.market_of(listed_on_b3, treasury), symbol))
            })
            .collect();
        Ok(tracked.into_iter().collect())
    }

    /// Registra o aporte e soma o que foi comprado em cada ativo, numa transação.
    pub async fn record_contribution(
        &self,
        user: Uuid,
        amount: Decimal,
        items: &[NewContributionItem],
    ) -> Result<Contribution, StoreError> {
        let mut tx = self.pool.begin().await?;
        let id = Uuid::new_v4();
        let (created_at,): (OffsetDateTime,) =
            sqlx::query_as("insert into contributions (id, user_id, amount) values ($1, $2, $3) returning created_at")
                .bind(id)
                .bind(user)
                .bind(amount)
                .fetch_one(&mut *tx)
                .await?;
        let mut stored = Vec::with_capacity(items.len());
        for item in items {
            let row: Option<AssetRow> = sqlx::query_as(concat!(
                "select ",
                asset_columns!(),
                " from assets where user_id = $1 and id = $2 for update"
            ))
            .bind(user)
            .bind(item.asset_id)
            .fetch_optional(&mut *tx)
            .await?;
            let asset = Asset::try_from(row.ok_or(StoreError::NotFound)?)?;
            let value = round_cents(item.quantity * item.unit_price);
            if item.quantity <= Decimal::ZERO
                || item.unit_price <= Decimal::ZERO
                || value <= Decimal::ZERO
            {
                return Err(StoreError::Invalid(
                    "quantidade e preco precisam ser positivos".into(),
                ));
            }
            let added = match asset.market().unit_kind() {
                UnitKind::Value => value,
                UnitKind::Whole | UnitKind::Fractional | UnitKind::Hundredth => item.quantity,
            };
            sqlx::query(
                "update assets set quantity = quantity + $2, updated_at = now() where id = $1",
            )
            .bind(asset.id)
            .bind(added)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "insert into contribution_items
                    (id, contribution_id, asset_id, symbol, display_name, quantity, unit_price, value)
                 values ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(asset.id)
            .bind(&asset.symbol)
            .bind(&asset.display_name)
            .bind(item.quantity)
            .bind(item.unit_price)
            .bind(value)
            .execute(&mut *tx)
            .await?;
            stored.push(ContributionItem {
                asset_id: Some(asset.id),
                symbol: asset.symbol,
                display_name: asset.display_name,
                quantity: item.quantity.normalize(),
                unit_price: item.unit_price.normalize(),
                value,
            });
        }
        tx.commit().await?;
        Ok(Contribution {
            id,
            created_at,
            amount,
            items: stored,
        })
    }

    pub async fn contributions(&self, user: Uuid) -> Result<Vec<Contribution>, StoreError> {
        let rows: Vec<ContributionRow> = sqlx::query_as(
            "select id, created_at, amount from contributions where user_id = $1
              order by created_at desc, id limit 200",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let items: Vec<ContributionItemRow> = sqlx::query_as(
            "select contribution_id, asset_id, symbol, display_name, quantity, unit_price, value
               from contribution_items where contribution_id = any($1) order by value desc, id",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        let mut by_contribution: HashMap<Uuid, Vec<ContributionItem>> = HashMap::new();
        for item in items {
            by_contribution
                .entry(item.contribution_id)
                .or_default()
                .push(ContributionItem {
                    asset_id: item.asset_id,
                    symbol: item.symbol,
                    display_name: item.display_name,
                    quantity: item.quantity.normalize(),
                    unit_price: item.unit_price.normalize(),
                    value: item.value,
                });
        }
        Ok(rows
            .into_iter()
            .map(|r| Contribution {
                items: by_contribution.remove(&r.id).unwrap_or_default(),
                id: r.id,
                created_at: r.created_at,
                amount: r.amount,
            })
            .collect())
    }
}

async fn seed_questions(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    checklist: Checklist,
) -> Result<(), sqlx::Error> {
    for (index, prompt) in defaults::questions(checklist).iter().enumerate() {
        sqlx::query(
            "insert into checklist_questions (id, user_id, checklist, position, prompt) values ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(checklist.as_str())
        .bind(i32::try_from(index + 1).unwrap_or(i32::MAX))
        .bind(*prompt)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
