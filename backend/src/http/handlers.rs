//! Um handler por rota. Validação aqui; regra de negócio em `service` e `domain`.

use std::collections::HashMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::dto::*;
use super::references::ceiling_choice;
use super::{treasury, ApiJson, ApiPath, ApiQuery, AppState, CurrentUser};
use crate::ceiling;
use crate::domain::allocation::{round_cents, Targets, FULL_BP};
use crate::domain::asset_class::{normalize_symbol, AssetClass, Checklist, Market, UnitKind};
use crate::domain::ceiling::CeilingChoice;
use crate::domain::income::today_in_brazil;
use crate::domain::ledger::{check_for_unit, Kind};
use crate::domain::treasury::Bond;
use crate::error::{ApiError, ApiResult};
use crate::service;
use crate::store::{AssetPatch, NewAsset, NewContributionItem, OpeningCost, StoreError};

/// Teto de valores aceitos (10^15), para número absurdo não virar overflow de `numeric`.
fn max_money() -> Decimal {
    Decimal::new(1_000_000_000_000_000, 0)
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Duplicate => {
                ApiError::asset_already_exists("Esse simbolo ja esta nesta classe.")
            }
            StoreError::NotFound => ApiError::not_found(),
            StoreError::Invalid(detail) => ApiError::validation(detail),
            StoreError::Ledger(error) => super::ledger::ledger_error(&error),
            StoreError::Db(error) => error.into(),
        }
    }
}

fn non_negative(name: &str, value: Decimal) -> ApiResult<Decimal> {
    if value < Decimal::ZERO || value > max_money() {
        return Err(ApiError::validation(format!(
            "{name} precisa estar entre 0 e 10^15."
        )));
    }
    Ok(value)
}

fn positive(name: &str, value: Decimal) -> ApiResult<Decimal> {
    if value <= Decimal::ZERO || value > max_money() {
        return Err(ApiError::validation(format!(
            "{name} precisa ser maior que zero."
        )));
    }
    Ok(value)
}

fn score_value(value: i64) -> ApiResult<u8> {
    u8::try_from(value)
        .ok()
        .filter(|v| *v <= 10)
        .ok_or_else(|| ApiError::validation("A nota vai de 0 a 10."))
}

fn text(name: &str, value: &str, max: usize) -> ApiResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max {
        return Err(ApiError::validation(format!(
            "{name} precisa ter de 1 a {max} caracteres."
        )));
    }
    Ok(trimmed.to_owned())
}

/// A marca "negociado na B3" como fica gravada. Ações Nacionais e FIIs são
/// sempre da B3; nas outras classes ela é a escolha da pessoa (ETF, BDR).
fn b3_listing(class: AssetClass, requested: Option<bool>) -> ApiResult<bool> {
    if class.accepts_b3_listing() {
        return Ok(requested.unwrap_or(false));
    }
    match requested {
        Some(false) => Err(ApiError::validation(format!(
            "{} sao sempre da B3.",
            class.label()
        ))),
        _ => Ok(true),
    }
}

const NO_OPENING: &str = "Sem quantidade nao ha custo de entrada para informar.";

/// O nome de um ativo cotado quando nem o usuário nem a cotação dão um: o
/// título do Tesouro se chama pelo tipo e o ano; o resto, pelo símbolo.
fn fallback_name(symbol: &str) -> String {
    Bond::from_symbol(symbol).map_or_else(|| symbol.to_owned(), |bond| bond.name())
}

fn checklist(value: &str) -> ApiResult<Checklist> {
    Checklist::parse(value).ok_or_else(ApiError::not_found)
}

/// O dia de hoje em Brasília.
fn today() -> time::Date {
    today_in_brazil(time::OffsetDateTime::now_utc())
}

async fn asset_response(state: &AppState, user: Uuid, id: Uuid) -> ApiResult<AssetDto> {
    let view = service::portfolio(&state.store, user).await?;
    let ceilings = ceiling::resolve_ceilings(&state.store, user, &view, today()).await?;
    view.asset(id)
        .map(|a| AssetDto::new(a, ceilings.get(&id).copied().unwrap_or_default()))
        .ok_or_else(ApiError::not_found)
}

pub async fn info(State(state): State<AppState>) -> Json<InfoResponse> {
    Json(InfoResponse {
        version: state.info.version.clone(),
        commit: state.info.commit.clone(),
    })
}

pub async fn me(user: CurrentUser) -> Json<MeResponse> {
    Json(MeResponse { email: user.email })
}

pub async fn portfolio(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<PortfolioDto>> {
    let view = service::portfolio(&state.store, user.id).await?;
    // O teto de hoje de quem escolheu um método (sem nenhum, nada a mais é lido).
    let ceilings = ceiling::resolve_ceilings(&state.store, user.id, &view, today()).await?;
    Ok(Json(PortfolioDto::new(&view, &ceilings)))
}

pub async fn set_goals(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<GoalsRequest>,
) -> ApiResult<Json<GoalsResponse>> {
    let mut targets = Targets::default();
    for (class, raw) in body.targets {
        let percent = match raw {
            serde_json::Value::Number(n) => n.to_string().parse::<Decimal>().ok(),
            _ => None,
        }
        .filter(|p| *p >= Decimal::ZERO && *p <= Decimal::ONE_HUNDRED && p.scale() <= 2)
        .ok_or_else(|| {
            ApiError::validation(format!(
                "Meta de {} precisa estar entre 0 e 100.",
                class.label()
            ))
        })?;
        let bp = (percent * Decimal::ONE_HUNDRED).to_u32().unwrap_or(0);
        targets.set(class, bp);
    }
    if targets.sum() != FULL_BP {
        let sum = Decimal::from(targets.sum()) / Decimal::ONE_HUNDRED;
        return Err(ApiError::targets_must_sum_100(format!(
            "A soma enviada foi {}%.",
            sum.normalize()
        )));
    }
    state.store.set_goals(user.id, targets).await?;
    let response = AssetClass::ALL
        .into_iter()
        .map(|c| (c, f64::from(targets.get(c)) / 100.0))
        .collect();
    Ok(Json(GoalsResponse { targets: response }))
}

pub async fn create_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<CreateAssetRequest>,
) -> ApiResult<impl IntoResponse> {
    let class = body.asset_class;
    let score_override = body.score_override.map(score_value).transpose()?;
    let treasury = body.treasury.unwrap_or(false);
    if treasury && !class.accepts_treasury() {
        return Err(ApiError::validation(
            "Titulo do Tesouro so cabe em Renda Fixa.",
        ));
    }
    if treasury && body.listed_on_b3 == Some(true) {
        return Err(ApiError::validation("Titulo do Tesouro nao e da B3."));
    }
    let listed_on_b3 = !treasury && b3_listing(class, body.listed_on_b3)?;
    let market = class.market_of(listed_on_b3, treasury);
    let cost;
    let new = match market.unit_kind() {
        UnitKind::Value => {
            let display_name = text(
                "O nome",
                body.display_name.as_deref().unwrap_or_default(),
                120,
            )?;
            let amount =
                non_negative("O valor", body.amount.or(body.quantity).unwrap_or_default())?;
            if body.manual_price.is_some() || body.average_price.is_some() {
                return Err(ApiError::validation(
                    "Renda fixa nao tem preco por unidade; informe o valor.",
                ));
            }
            // Sem preço por unidade, o teto não se aplica: a escolha recusa.
            ceiling_choice(body.ceiling.as_ref(), class, market)?;
            cost = match body.invested {
                None => OpeningCost::Unknown,
                Some(_) if amount.is_zero() => {
                    return Err(ApiError::validation(NO_OPENING));
                }
                Some(invested) => OpeningCost::Invested(positive("O valor aplicado", invested)?),
            };
            NewAsset {
                asset_class: class,
                symbol: None,
                display_name,
                quantity: amount,
                manual_price: None,
                score_override,
                listed_on_b3,
                treasury,
                ceiling: CeilingChoice::None,
            }
        }
        UnitKind::Whole | UnitKind::Fractional | UnitKind::Hundredth => {
            let symbol = body
                .symbol
                .as_deref()
                .and_then(normalize_symbol)
                .ok_or_else(|| ApiError::validation("Informe um simbolo valido, como KNRI11."))?;
            if treasury && !Bond::is_symbol(&symbol) {
                return Err(ApiError::validation(
                    "Escolha o titulo na busca: a chave dele tem a forma TD-IPCA-20350515.",
                ));
            }
            let quantity = non_negative("A quantidade", body.quantity.unwrap_or_default())?;
            // Cota inteira na B3 e nos EUA, centésimos no Tesouro: a mesma
            // regra que vale para cada lançamento do livro.
            check_for_unit(Kind::Opening, quantity, None, market.unit_kind())
                .map_err(ApiError::validation)?;
            let manual_price = body
                .manual_price
                .map(|p| positive("O preco manual", p))
                .transpose()?;
            let ceiling = ceiling_choice(body.ceiling.as_ref(), class, market)?;
            if body.invested.is_some() {
                return Err(ApiError::validation(
                    "Ativo com cotacao tem preco medio (average_price), nao valor aplicado.",
                ));
            }
            cost = match body.average_price {
                None => OpeningCost::Unknown,
                Some(_) if quantity.is_zero() => {
                    return Err(ApiError::validation(NO_OPENING));
                }
                Some(price) => OpeningCost::AveragePrice(positive("O preco medio", price)?),
            };
            let quote = state.quotes.quote_now(market, &symbol).await;
            let display_name = match body
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                Some(name) => text("O nome", name, 120)?,
                None => quote
                    .and_then(|q| q.name)
                    .unwrap_or_else(|| fallback_name(&symbol))
                    .chars()
                    .take(120)
                    .collect(),
            };
            NewAsset {
                asset_class: class,
                symbol: Some(symbol),
                display_name,
                quantity,
                manual_price,
                score_override,
                listed_on_b3,
                treasury,
                ceiling,
            }
        }
    };
    let asset = state
        .store
        .insert_asset_with_opening(user.id, &new, cost)
        .await?;
    // O histórico de dividendos chega em segundo plano, para a estimativa de
    // renda já o encontrar; a resposta não espera.
    state.income.prefetch(market, asset.symbol.as_deref());
    state.icons.prefetch(market, asset.symbol.as_deref());
    state.fundamentals.prefetch(market, asset.symbol.as_deref());
    let dto = asset_response(&state, user.id, asset.id).await?;
    Ok((StatusCode::CREATED, Json(dto)))
}

pub async fn update_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<UpdateAssetRequest>,
) -> ApiResult<Json<AssetDto>> {
    let asset = state
        .store
        .asset(user.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if asset.treasury && body.listed_on_b3 == Some(true) {
        return Err(ApiError::validation("Titulo do Tesouro nao e da B3."));
    }
    let listed_on_b3 = match body.listed_on_b3 {
        None => asset.listed_on_b3,
        requested => b3_listing(asset.asset_class, requested)?,
    };
    let market = asset.asset_class.market_of(listed_on_b3, asset.treasury);
    let kind = market.unit_kind();
    // Renda fixa por valor não tem ticker, e com ticker não tem valor
    // informado: trocar um pelo outro é outro ativo, não uma edição.
    if (kind == UnitKind::Value) != (asset.market().unit_kind() == UnitKind::Value) {
        return Err(ApiError::validation(
            "Renda fixa com ticker e renda fixa por valor sao cadastros diferentes: remova o ativo e adicione de novo.",
        ));
    }
    let quantity = match (kind, body.quantity, body.amount) {
        (UnitKind::Value, q, a) => a.or(q).map(|v| non_negative("O valor", v)).transpose()?,
        (_, Some(_), Some(_)) | (_, None, Some(_)) => {
            return Err(ApiError::validation(
                "Use quantity para ativos com cotacao.",
            ));
        }
        (_, q, None) => q.map(|v| non_negative("A quantidade", v)).transpose()?,
    };
    // Vale para a quantidade que fica: ligar a B3 num cripto com 0,5 unidade
    // pede a quantidade inteira junto.
    check_for_unit(
        Kind::Adjustment,
        quantity.unwrap_or(asset.quantity),
        None,
        kind,
    )
    .map_err(ApiError::validation)?;
    if kind == UnitKind::Value && body.manual_price.is_some_and(|p| p.is_some()) {
        return Err(ApiError::validation(
            "Renda fixa nao tem preco por unidade; informe o valor.",
        ));
    }
    let ceiling = body
        .ceiling
        .as_ref()
        .map(|request| ceiling_choice(request.as_ref(), asset.asset_class, market))
        .transpose()?;
    let patch = AssetPatch {
        listed_on_b3: body.listed_on_b3.map(|_| listed_on_b3),
        quantity,
        display_name: body
            .display_name
            .as_deref()
            .map(|n| text("O nome", n, 120))
            .transpose()?,
        manual_price: body
            .manual_price
            .map(|p| p.map(|v| positive("O preco manual", v)).transpose())
            .transpose()?,
        score_override: body
            .score_override
            .map(|s| s.map(score_value).transpose())
            .transpose()?,
        ceiling,
        tax_instrument: body.tax_instrument,
    };
    state
        .store
        .update_asset(user.id, id, &patch)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if let Some(answers) = body.answers {
        let answers: Vec<(Uuid, Option<bool>)> = answers.into_iter().collect();
        state.store.set_answers(user.id, &asset, &answers).await?;
    }
    // Mercado novo, símbolo novo no provedor: cota já, para a resposta vir com
    // preço em vez de esperar a próxima rodada.
    if market != asset.market() {
        if let Some(symbol) = asset.symbol.as_deref() {
            state.quotes.quote_now(market, symbol).await;
        }
        state.income.prefetch(market, asset.symbol.as_deref());
        state.icons.prefetch(market, asset.symbol.as_deref());
        state.fundamentals.prefetch(market, asset.symbol.as_deref());
    }
    Ok(Json(asset_response(&state, user.id, id).await?))
}

pub async fn delete_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<StatusCode> {
    if state.store.delete_asset(user.id, id).await? {
        // O streamer deixa de acompanhar o símbolo, se ninguém mais o tem.
        state.quotes.live().resync();
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

pub async fn add_question(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(name): ApiPath<String>,
    ApiJson(body): ApiJson<QuestionRequest>,
) -> ApiResult<impl IntoResponse> {
    let checklist = checklist(&name)?;
    let prompt = text("A pergunta", &body.prompt, 300)?;
    let question = state
        .store
        .add_question(user.id, checklist, &prompt)
        .await?;
    Ok((StatusCode::CREATED, Json(QuestionDto::from(&question))))
}

pub async fn reset_checklist(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(name): ApiPath<String>,
) -> ApiResult<Json<Vec<QuestionDto>>> {
    let checklist = checklist(&name)?;
    let questions = state.store.reset_checklist(user.id, checklist).await?;
    Ok(Json(questions.iter().map(QuestionDto::from).collect()))
}

pub async fn update_question(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<QuestionRequest>,
) -> ApiResult<Json<QuestionDto>> {
    let prompt = text("A pergunta", &body.prompt, 300)?;
    let question = state
        .store
        .update_question(user.id, id, &prompt)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(QuestionDto::from(&question)))
}

pub async fn delete_question(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<StatusCode> {
    if state.store.delete_question(user.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

pub async fn plan(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<PlanRequest>,
) -> ApiResult<Json<PlanDto>> {
    let amount = non_negative("O valor do aporte", body.amount)?;
    let (plan, view) = service::plan(&state.store, user.id, amount).await?;
    // Só o cache: o plano não espera o provedor de dividendos.
    let today = today();
    let income = state.income.estimate(&view, today).await;
    // O aviso de quem está sem teto: só o que já está guardado, nunca a rede.
    let references = ceiling::plan_references(&state.store, user.id, &plan, &view, today).await?;
    Ok(Json(
        PlanDto::new(&plan, &view)
            .with_income(&income)
            .with_price_references(&references),
    ))
}

pub async fn record_contribution(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<ContributionRequest>,
) -> ApiResult<impl IntoResponse> {
    let amount = non_negative("O valor do aporte", body.amount)?;
    let cash_used = round_cents(non_negative(
        "O caixa usado",
        body.cash_used.unwrap_or(Decimal::ZERO),
    )?);
    if body.items.is_empty() {
        return Err(ApiError::validation(
            "O aporte precisa de pelo menos um item.",
        ));
    }
    let mut seen: HashMap<Uuid, ()> = HashMap::new();
    let mut items = Vec::with_capacity(body.items.len());
    for item in body.items {
        if seen.insert(item.asset_id, ()).is_some() {
            return Err(ApiError::validation(
                "Cada ativo aparece uma vez no aporte.",
            ));
        }
        items.push(NewContributionItem {
            asset_id: item.asset_id,
            quantity: positive("A quantidade", item.quantity)?,
            unit_price: positive("O preco", item.unit_price)?,
        });
    }
    let contribution = state
        .store
        .record_contribution(user.id, amount, cash_used, &items)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(ContributionDto::from(&contribution)),
    ))
}

pub async fn contributions(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<ContributionDto>>> {
    let list = state.store.contributions(user.id).await?;
    Ok(Json(list.iter().map(ContributionDto::from).collect()))
}

pub async fn symbols(
    State(state): State<AppState>,
    _user: CurrentUser,
    ApiQuery(query): ApiQuery<SymbolQuery>,
) -> ApiResult<Json<Vec<SymbolDto>>> {
    let term = query.query.trim();
    if term.is_empty() || term.chars().count() > 40 {
        return Err(ApiError::validation(
            "A busca precisa ter de 1 a 40 caracteres.",
        ));
    }
    // Os títulos do Tesouro vêm do banco e vão na frente; o provedor completa.
    // Se ele cair e o Tesouro já respondeu, a pessoa ainda escolhe o título.
    let mut found = treasury::suggestions(&state.store, term).await?;
    match state.quotes.provider().search(term).await {
        Ok(matches) => found.extend(matches.into_iter().map(|m| SymbolDto {
            symbol: m.symbol,
            display_name: m.display_name,
            exchange: m.exchange,
            suggested_class: m.suggested_class,
            listed_on_b3: m.listed_on_b3,
            treasury: false,
        })),
        Err(error) => {
            tracing::warn!(error = %error, "busca de simbolo falhou");
            if found.is_empty() {
                return Err(ApiError::quote_provider_unavailable());
            }
        }
    }
    Ok(Json(found))
}

pub async fn refresh_quotes(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<RefreshDto>> {
    if !state.take_refresh_slot(user.id).await {
        return Ok(Json(RefreshDto {
            refreshed: 0,
            failed: Vec::new(),
        }));
    }
    let targets: Vec<(Market, String)> = state
        .store
        .assets(user.id)
        .await?
        .into_iter()
        .filter_map(|a| {
            let market = a.market();
            a.symbol.map(|s| (market, s))
        })
        .collect();
    let report = state.quotes.refresh(&targets).await;
    Ok(Json(RefreshDto {
        refreshed: report.refreshed,
        failed: report.failed,
    }))
}
