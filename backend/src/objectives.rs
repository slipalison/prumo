//! Apuração dos objetivos em segundo plano: quem passou da meta ganha o
//! `achieved_at` no dia, sem depender de alguém abrir uma tela. A tarefa diária
//! de desempenho chama [`record_achieved`] já com o lock de job, e o `update`
//! só vale onde `achieved_at is null`, então duas réplicas (ou a tela, que faz
//! a mesma apuração ao ler) nunca gravam duas datas.
//!
//! Aqui também mora o progresso da meta de renda nos dois modos (média e
//! piso), que a tela Renda e a de Objetivos leem igual.

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::domain::asset_class::AssetClass;
use crate::domain::income::{summarize, today_in_brazil, Entry, GoalProgress, ObjectiveKind};
use crate::domain::objectives::is_reached;
use crate::domain::projection::goal::IncomeMode;
use crate::domain::projection::progress::{progress, received_floor, Estimated, Received};
use crate::income::PortfolioIncome;
use crate::projection::portfolio_shape;
use crate::service::{self, PortfolioView};
use crate::store::{IncomeEvent, Objective, Store, StoreError};

/// O que o objetivo de patrimônio ou de classe mede na carteira de agora.
/// `None` na meta de renda mensal, que se mede pelo livro de proventos.
pub fn current_value(view: &PortfolioView, objective: &Objective) -> Option<Decimal> {
    match (objective.kind, objective.asset_class) {
        (ObjectiveKind::NetWorth, _) => Some(view.total_value),
        (ObjectiveKind::ClassValue, Some(class)) => Some(class_value(view, class)),
        _ => None,
    }
}

/// O valor de uma classe na carteira de agora.
pub fn class_value(view: &PortfolioView, class: AssetClass) -> Decimal {
    view.classes
        .iter()
        .find(|c| c.asset_class == class)
        .map_or(Decimal::ZERO, |c| c.current_value)
}

/// O recebido, como a meta de renda lê: a média dos 12 meses e o piso dos
/// 12 fechados.
pub fn received_income(entries: &[Entry], today: Date) -> Received {
    Received {
        average: summarize(entries, today).monthly_average,
        floor: received_floor(entries, today),
    }
}

/// A estimativa, como a meta de renda lê: a da Renda e o piso pelo
/// calendário da carteira.
pub fn estimated_income(income: &PortfolioIncome) -> Estimated {
    Estimated::of(income.monthly_total, &portfolio_shape(income))
}

/// O progresso da meta de renda no modo dela.
pub fn income_progress(
    objective: &Objective,
    received: Received,
    estimated: Estimated,
) -> GoalProgress {
    progress(
        objective.income_mode.unwrap_or_default(),
        objective.target,
        received,
        estimated,
    )
}

/// Se a meta de renda precisa da estimativa: só quando nada foi recebido na
/// janela que o modo dela olha.
pub fn needs_estimate(objective: &Objective, received: Received) -> bool {
    match objective.income_mode.unwrap_or_default() {
        IncomeMode::Average => received.average <= Decimal::ZERO,
        IncomeMode::Floor => received.floor.is_none(),
    }
}

/// Grava a data de conquista dos objetivos abertos da pessoa que já chegaram à
/// meta (patrimônio, classe e renda mensal). Devolve quantos foram gravados
/// agora. A meta de renda só conquista com proventos recebidos, nunca pela
/// estimativa, como na tela Renda.
pub async fn record_achieved_for(
    store: &Store,
    user: Uuid,
    now: OffsetDateTime,
) -> Result<usize, StoreError> {
    let open = store.open_objectives(user).await?;
    if open.is_empty() {
        return Ok(0);
    }
    let view = if open.iter().any(|o| o.kind != ObjectiveKind::MonthlyIncome) {
        Some(service::portfolio(store, user).await?)
    } else {
        None
    };
    let received = if open.iter().any(|o| o.kind == ObjectiveKind::MonthlyIncome) {
        let ledger = store.income_ledger(user).await?;
        let entries: Vec<_> = ledger.iter().map(IncomeEvent::entry).collect();
        Some(received_income(&entries, today_in_brazil(now)))
    } else {
        None
    };

    let mut recorded = 0;
    for objective in open {
        let reached = match (objective.kind, &view, received) {
            (ObjectiveKind::MonthlyIncome, _, Some(received)) => {
                income_progress(&objective, received, Estimated::default()).reached
            }
            (_, Some(view), _) => current_value(view, &objective)
                .is_some_and(|current| is_reached(current, objective.target)),
            _ => false,
        };
        if reached && store.achieve_objective(user, objective.id, now).await? {
            recorded += 1;
        }
    }
    Ok(recorded)
}

/// A apuração de todas as pessoas com objetivo aberto. Falha numa pessoa é
/// registrada e não impede as outras; o que falhou é refeito na próxima rodada.
pub async fn record_achieved(store: &Store, now: OffsetDateTime) -> Result<usize, StoreError> {
    let mut total = 0;
    for user in store.users_with_open_objectives().await? {
        match record_achieved_for(store, user, now).await {
            Ok(count) => total += count,
            Err(error) => {
                tracing::error!(error = %error, "objetivos da carteira nao apurados");
            }
        }
    }
    Ok(total)
}
