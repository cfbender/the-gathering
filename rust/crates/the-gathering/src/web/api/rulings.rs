//! AI rulings at the table, their OpenRouter settings, and the Comprehensive Rules they
//! look rules up in.

use std::sync::LazyLock;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ai::openrouter::CheckError;
use crate::ai::ruling::{self, AskError, Question, Turn};
use crate::ai::{
    AiSettings, budget_month, save_settings, settings, spent_this_month, usage_this_month,
};
use crate::db::UtcDateTime;
use crate::error::{ApiError, ApiResult};
use crate::patch::Patch;
use crate::regex::{Regex, compile};
use crate::rules;
use crate::state::AppState;
use crate::validation::Validator;
use crate::web::auth::AuthUser;
use crate::web::extract::JsonBody;

use super::{check_user_limit, data};

/// An OpenRouter model id, such as `anthropic/claude-sonnet-5.5` or `openai/gpt-5:online`.
static MODEL_ID: LazyLock<Regex> = LazyLock::new(|| compile(r"^[A-Za-z0-9][A-Za-z0-9._:/~-]*$"));

/// Characters a question may have.
const MAX_QUESTION: usize = 2_000;
/// Characters an earlier answer sent back may have.
const MAX_ANSWER: usize = 20_000;
/// Table cards sent as context.
const MAX_TABLE_CARDS: usize = 100;

fn detail(status: StatusCode, message: &str) -> ApiError {
    ApiError::Custom(status, json!({ "errors": { "detail": message } }))
}

/// `GET /api/ai-rulings`: whether rulings are available, the model, the rules release, and
/// the member's spending this month against the limit.
pub async fn show(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Value>> {
    let settings = settings(&state.pool, &state.config.secret_key).await?;
    let rules = rules::status(&state.pool).await?;
    let (_, resets_on) = budget_month(UtcDateTime::now());
    Ok(data(json!({
        "enabled": settings.credentials().is_some(),
        "model": settings.model,
        "rules_effective_on": rules.effective_on,
        "usage": {
            "spent_usd": spent_this_month(&state.pool, user.id).await?,
            "limit_usd": settings.monthly_limit_usd,
            "resets_on": resets_on,
        },
    })))
}

/// An earlier turn of the conversation.
#[derive(Debug, Deserialize)]
pub struct TurnInput {
    question: String,
    answer: String,
}

/// `POST /api/ai-rulings` body.
#[derive(Debug, Deserialize)]
pub struct RulingRequest {
    question: Option<String>,
    /// Earlier turns, oldest first; only the last few are sent to the model.
    #[serde(default)]
    history: Vec<TurnInput>,
    /// Names of cards in play at the table.
    #[serde(default)]
    cards: Vec<String>,
}

/// `POST /api/ai-rulings`: answers a rules question (rate limited per member).
pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    JsonBody(request): JsonBody<RulingRequest>,
) -> ApiResult<Json<Value>> {
    let mut cs = Validator::new();
    let question = request.question.as_deref().map(str::trim);
    cs.required("question", question);
    cs.length("question", question, None, Some(MAX_QUESTION));
    let history_start = request.history.len().saturating_sub(ruling::HISTORY_TURNS);
    let history = request.history.get(history_start..).unwrap_or_default();
    if history.iter().any(|turn| {
        turn.question.chars().count() > MAX_QUESTION || turn.answer.chars().count() > MAX_ANSWER
    }) {
        cs.add_error("history", "is too long");
    }
    cs.finish()?;
    check_user_limit(
        &state,
        "ai_rulings",
        user.id,
        state.config.rate_limits.ai_rulings,
    )?;
    let question = Question {
        text: question.unwrap_or_default().to_owned(),
        history: history
            .iter()
            .map(|turn| Turn {
                question: turn.question.clone(),
                answer: turn.answer.clone(),
            })
            .collect(),
        table_cards: request
            .cards
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty() && name.chars().count() <= 200)
            .take(MAX_TABLE_CARDS)
            .map(str::to_owned)
            .collect(),
    };
    match ruling::ask(&state, user.id, &question).await {
        Ok(answer) => Ok(data(json!(answer))),
        Err(AskError::NotConfigured) => Err(detail(
            StatusCode::SERVICE_UNAVAILABLE,
            "AI rulings are not set up on this server. An administrator can add an OpenRouter API key and model under Admin › Settings.",
        )),
        Err(AskError::OverBudget {
            limit_usd,
            resets_on,
        }) => Err(ApiError::Custom(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "errors": {
                "code": "ai_budget_exhausted",
                "detail": format!(
                    "You have used this month's ${limit_usd:.2} of AI rulings. Your budget resets on {resets_on}."
                ),
            } }),
        )),
        Err(AskError::Chat(error)) => {
            tracing::warn!("AI ruling failed: {error}");
            Err(detail(StatusCode::BAD_GATEWAY, &error.0))
        }
        Err(AskError::Database(error)) => Err(error.into()),
    }
}

async fn settings_json(state: &AppState, settings: &AiSettings) -> ApiResult<Json<Value>> {
    let (month, _) = budget_month(UtcDateTime::now());
    Ok(data(json!({
        "provider": "openrouter",
        "model": settings.model,
        "has_api_key": settings.api_key.is_some(),
        "monthly_limit_usd": settings.monthly_limit_usd,
        "usage_month": month,
        "usage": usage_this_month(&state.pool).await?,
    })))
}

/// `GET /api/admin/ai-settings`: the provider, model, whether a key is stored (never the
/// key), the monthly limit, and each member's spending this month.
pub async fn settings_show(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    settings_json(
        &state,
        &settings(&state.pool, &state.config.secret_key).await?,
    )
    .await
}

/// `PATCH /api/admin/ai-settings` body.
#[derive(Debug, Default, Deserialize)]
pub struct SettingsUpdate {
    /// OpenRouter model id; `null` or blank clears it.
    #[serde(default)]
    model: Patch<String>,
    /// OpenRouter API key; `null` or blank removes it.
    #[serde(default)]
    api_key: Patch<String>,
    /// Most each member may spend per calendar month, in USD; `null` for no limit.
    #[serde(default)]
    monthly_limit_usd: Patch<f64>,
}

/// `PATCH /api/admin/ai-settings`: stores the model and key after OpenRouter confirms the
/// key works and offers the model with tool calling.
pub async fn settings_update(
    State(state): State<AppState>,
    JsonBody(update): JsonBody<SettingsUpdate>,
) -> ApiResult<Json<Value>> {
    let current = settings(&state.pool, &state.config.secret_key).await?;
    let api_key = update.api_key.trimmed();
    let model = update.model.trimmed().or(current.model.clone());
    let mut cs = Validator::new();
    cs.length("model", model.as_deref(), None, Some(200));
    cs.format(
        "model",
        model.as_deref(),
        &MODEL_ID,
        "should be an OpenRouter model ID, such as anthropic/claude-sonnet-5.5",
    );
    if let Patch::Set(Some(key)) = &api_key {
        cs.length("api_key", Some(key), None, Some(500));
    }
    let limit = update.monthly_limit_usd.or(current.monthly_limit_usd);
    if limit.is_some_and(|limit| !(0.0..=10_000.0).contains(&limit)) {
        cs.add_error("monthly_limit_usd", "must be between 0 and 10000");
    }
    cs.finish()?;
    let candidate = AiSettings {
        api_key: api_key.clone().or(current.api_key.clone()),
        model: model.clone(),
        monthly_limit_usd: limit,
    };
    // Only a new key or model needs OpenRouter's confirmation.
    let changed = candidate.api_key != current.api_key || candidate.model != current.model;
    if changed && let Some((key, model)) = candidate.credentials() {
        crate::ai::openrouter::check(&state.http, &state.config.openrouter_api_base, key, model)
            .await
            .map_err(|error| match error {
                CheckError::Invalid(errors) => ApiError::Validation(errors),
                CheckError::Unreachable(message) => detail(StatusCode::BAD_GATEWAY, &message),
            })?;
    }
    save_settings(
        &state.pool,
        &state.config.secret_key,
        &api_key,
        model.as_deref(),
        limit,
    )
    .await?;
    settings_json(&state, &candidate).await
}

/// `GET /api/admin/rules`: the installed Comprehensive Rules and the last update check.
pub async fn rules_show(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(data(json!(rules::status(&state.pool).await?)))
}

/// `POST /api/admin/rules/sync`: checks Wizards' rules page now and installs a newer
/// release. Answers with the resulting status, or a 502 naming why the check failed.
pub async fn rules_sync(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let (outcome, status) = state.rules_sync.check(&state, false).await;
    match outcome {
        Ok(outcome) => Ok(data(json!({
            "outcome": outcome,
            "status": status,
        }))),
        Err(error) => Err(detail(
            StatusCode::BAD_GATEWAY,
            &format!("Could not update the Comprehensive Rules: {error}"),
        )),
    }
}
