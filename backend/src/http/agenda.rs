//! `GET /api/v1/agenda`: a linha do tempo dos ativos da carteira num período
//! (proventos, resultado entregue à CVM, fato relevante, comunicados, dias sem
//! pregão), e `GET /api/v1/agenda/unusual-income`: os proventos a receber fora
//! do padrão, para o selo da Renda. A regra está em `domain::agenda`; a
//! leitura, em `crate::agenda`; o contrato, em `docs/api.md`.

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

use super::dto::decimal_out;
use super::{ApiQuery, AppState, CurrentUser};
use crate::agenda::{self, UnusualIncome};
use crate::domain::agenda::unusual::{Direction, Reference, Unusual};
use crate::domain::agenda::{
    valid_range, Detail, Document, DocumentCategory, DocumentSource, Event, EventKind,
    IncomeDetail, IncomeState, UnusualDetail, UnusualMark, MAX_RANGE_DAYS,
};
use crate::domain::asset_class::AssetClass;
use crate::domain::fundamentals::FilingKind;
use crate::domain::income::{today_in_brazil, IncomeKind, IncomeSource};
use crate::domain::tax::calendar::Closure;
use crate::error::{ApiError, ApiResult};
use crate::store::Asset;

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

/// Sem período na consulta: de 30 dias atrás a 90 dias à frente.
const DEFAULT_BACK: i64 = 30;
const DEFAULT_AHEAD: i64 = 90;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgendaQuery {
    #[serde(default, with = "iso_date::option")]
    pub from: Option<Date>,
    #[serde(default, with = "iso_date::option")]
    pub to: Option<Date>,
}

#[derive(Debug, Serialize)]
pub struct EventAssetDto {
    pub asset_id: Uuid,
    pub symbol: Option<String>,
    pub display_name: String,
    pub asset_class: AssetClass,
}

impl From<&Asset> for EventAssetDto {
    fn from(asset: &Asset) -> Self {
        Self {
            asset_id: asset.id,
            symbol: asset.symbol.clone(),
            display_name: asset.display_name.clone(),
            asset_class: asset.asset_class,
        }
    }
}

/// O habitual de um período.
#[derive(Debug, Serialize)]
pub struct ReferenceDto {
    /// A média dos pagamentos parecidos, por cota de hoje.
    #[serde(with = "decimal_out")]
    pub usual: Decimal,
    /// `valor ÷ habitual − 1` (`-0.25` é 25% abaixo).
    #[serde(with = "decimal_out")]
    pub change: Decimal,
    pub payments: usize,
}

impl From<&Reference> for ReferenceDto {
    fn from(r: &Reference) -> Self {
        Self {
            usual: r.usual,
            change: r.change,
            payments: r.payments,
        }
    }
}

/// A marca de fora do padrão.
#[derive(Debug, Serialize)]
pub struct UnusualMarkDto {
    pub direction: Direction,
    /// Os 12 meses antes da data ex.
    pub recent: ReferenceDto,
    /// Até 5 anos antes; `null` com menos de 2 anos de histórico.
    pub long: Option<ReferenceDto>,
    /// Quantos anos o período longo cobre (2 a 5), ou 0.
    pub years: u32,
}

impl From<&UnusualMark> for UnusualMarkDto {
    fn from(m: &UnusualMark) -> Self {
        Self {
            direction: m.direction,
            recent: (&m.recent).into(),
            long: m.long.as_ref().map(ReferenceDto::from),
            years: m.years,
        }
    }
}

impl From<&Unusual> for UnusualMarkDto {
    fn from(u: &Unusual) -> Self {
        (&UnusualMark::from(u)).into()
    }
}

#[derive(Debug, Serialize)]
pub struct IncomeDto {
    /// O provento da Renda; `null` no anúncio ainda sem data de pagamento.
    pub income_id: Option<Uuid>,
    pub kind: IncomeKind,
    #[serde(with = "iso_date::option")]
    pub record_date: Option<Date>,
    #[serde(with = "iso_date::option")]
    pub ex_date: Option<Date>,
    #[serde(with = "iso_date::option")]
    pub pay_date: Option<Date>,
    #[serde(with = "decimal_out")]
    pub amount_per_unit: Decimal,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    #[serde(with = "decimal_out")]
    pub gross: Decimal,
    /// O líquido da Renda (estimado no "a definir").
    #[serde(with = "decimal_out")]
    pub net: Decimal,
    pub state: IncomeState,
    pub source: IncomeSource,
    pub unusual: Option<UnusualMarkDto>,
}

impl From<&IncomeDetail> for IncomeDto {
    fn from(d: &IncomeDetail) -> Self {
        Self {
            income_id: d.income_id,
            kind: d.kind,
            record_date: d.record_date,
            ex_date: d.ex_date,
            pay_date: d.pay_date,
            amount_per_unit: d.per_unit,
            quantity: d.quantity,
            gross: d.gross,
            net: d.net,
            state: d.state,
            source: d.source,
            unusual: d.unusual.as_ref().map(UnusualMarkDto::from),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UnusualDto {
    pub direction: Direction,
    /// O que a data com pagou por cota de hoje, bruto.
    #[serde(with = "decimal_out")]
    pub per_unit: Decimal,
    #[serde(with = "iso_date")]
    pub record_date: Date,
    #[serde(with = "iso_date")]
    pub ex_date: Date,
    #[serde(with = "iso_date::option")]
    pub approved_on: Option<Date>,
    #[serde(serialize_with = "dates")]
    pub pay_dates: Vec<Date>,
    pub recent: ReferenceDto,
    pub long: Option<ReferenceDto>,
    pub years: u32,
}

fn dates<S: serde::Serializer>(list: &[Date], serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(list.len()))?;
    for day in list {
        seq.serialize_element(&day.to_string())?;
    }
    seq.end()
}

impl From<&UnusualDetail> for UnusualDto {
    fn from(d: &UnusualDetail) -> Self {
        Self {
            direction: d.unusual.direction,
            per_unit: d.unusual.per_unit,
            record_date: d.record_date,
            ex_date: d.ex_date,
            approved_on: d.approved_on,
            pay_dates: d.pay_dates.clone(),
            recent: (&d.unusual.recent).into(),
            long: d.unusual.long.as_ref().map(ReferenceDto::from),
            years: d.unusual.years,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DocumentDto {
    pub category: DocumentCategory,
    pub source: DocumentSource,
    /// Tipo e espécie como a fonte escreve.
    pub title: Option<String>,
    pub subject: Option<String>,
    #[serde(with = "iso_date::option")]
    pub reference_date: Option<Date>,
    pub version: i32,
    /// O documento original (RAD da CVM ou Fundos.NET).
    pub url: Option<String>,
    /// `annual` (DFP) ou `quarterly` (ITR), no resultado da companhia.
    pub filing: Option<FilingKind>,
}

impl From<&Document> for DocumentDto {
    fn from(d: &Document) -> Self {
        Self {
            category: d.category,
            source: d.source,
            title: d.title.clone(),
            subject: d.subject.clone(),
            reference_date: d.reference_date,
            version: d.version,
            url: d.url.clone(),
            filing: d.filing,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EventDto {
    #[serde(with = "iso_date")]
    pub date: Date,
    #[serde(rename = "type")]
    pub kind: EventKind,
    pub assets: Vec<EventAssetDto>,
    pub income: Option<IncomeDto>,
    pub unusual: Option<UnusualDto>,
    pub document: Option<DocumentDto>,
    pub closure: Option<Closure>,
}

fn event_dto(event: &Event, agenda: &agenda::Agenda) -> EventDto {
    let mut assets: Vec<EventAssetDto> = event
        .assets
        .iter()
        .filter_map(|id| agenda.assets.get(id).map(EventAssetDto::from))
        .collect();
    assets.sort_by(|a, b| {
        a.symbol
            .cmp(&b.symbol)
            .then(a.display_name.cmp(&b.display_name))
    });
    let (mut income, mut unusual, mut document, mut closure) = (None, None, None, None);
    match &event.detail {
        Detail::Income(d) => income = Some(d.into()),
        Detail::Unusual(d) => unusual = Some(d.into()),
        Detail::Document(d) => document = Some(d.into()),
        Detail::Closed(c) => closure = Some(*c),
    }
    EventDto {
        date: event.date,
        kind: event.kind,
        assets,
        income,
        unusual,
        document,
        closure,
    }
}

#[derive(Debug, Serialize)]
pub struct FreshnessDto {
    /// A conferência mais recente dos documentos das companhias (CVM, semanal).
    #[serde(with = "time::serde::rfc3339::option")]
    pub cvm_documents: Option<OffsetDateTime>,
    /// A busca mais velha dos documentos dos FIIs da carteira (Fundos.NET).
    #[serde(with = "time::serde::rfc3339::option")]
    pub fund_documents: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize)]
pub struct AgendaDto {
    #[serde(with = "iso_date")]
    pub today: Date,
    #[serde(with = "iso_date")]
    pub from: Date,
    #[serde(with = "iso_date")]
    pub to: Date,
    /// O último ano com calendário de pregões; depois dele, a Agenda não marca
    /// dia sem pregão.
    pub calendar_until: Option<i32>,
    pub updated: FreshnessDto,
    pub events: Vec<EventDto>,
}

pub async fn show(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<AgendaQuery>,
) -> ApiResult<Json<AgendaDto>> {
    let today = today_in_brazil(OffsetDateTime::now_utc());
    let from = query
        .from
        .unwrap_or_else(|| today.saturating_sub(Duration::days(DEFAULT_BACK)));
    let to = query
        .to
        .unwrap_or_else(|| today.saturating_add(Duration::days(DEFAULT_AHEAD)));
    if !valid_range(from, to) {
        return Err(ApiError::validation(format!(
            "O periodo precisa comecar antes de terminar e ter no maximo {MAX_RANGE_DAYS} dias."
        )));
    }
    let agenda = agenda::agenda(&state.store, user.id, today, from, to).await?;
    Ok(Json(AgendaDto {
        today,
        from,
        to,
        calendar_until: agenda.calendar_until,
        updated: FreshnessDto {
            cvm_documents: agenda.freshness.cvm_documents,
            fund_documents: agenda.freshness.fund_documents,
        },
        events: agenda
            .events
            .iter()
            .map(|e| event_dto(e, &agenda))
            .collect(),
    }))
}

#[derive(Debug, Serialize)]
pub struct UnusualIncomeDto {
    pub income_id: Uuid,
    pub asset_id: Uuid,
    #[serde(with = "iso_date")]
    pub record_date: Date,
    #[serde(with = "iso_date")]
    pub ex_date: Date,
    #[serde(with = "decimal_out")]
    pub per_unit: Decimal,
    #[serde(flatten)]
    pub mark: UnusualMarkDto,
}

impl From<&UnusualIncome> for UnusualIncomeDto {
    fn from(u: &UnusualIncome) -> Self {
        Self {
            income_id: u.income_id,
            asset_id: u.asset_id,
            record_date: u.record_date,
            ex_date: u.ex_date,
            per_unit: u.unusual.per_unit,
            mark: (&u.unusual).into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UnusualIncomeListDto {
    pub items: Vec<UnusualIncomeDto>,
}

pub async fn unusual_income(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<UnusualIncomeListDto>> {
    let today = today_in_brazil(OffsetDateTime::now_utc());
    let items = agenda::unusual_income(&state.store, user.id, today).await?;
    Ok(Json(UnusualIncomeListDto {
        items: items.iter().map(UnusualIncomeDto::from).collect(),
    }))
}
