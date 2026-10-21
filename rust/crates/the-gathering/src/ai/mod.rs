//! AI rulings: answers to Magic rules questions from a model on OpenRouter that looks
//! rules up in the installed Comprehensive Rules and cards up in the local catalog.
//!
//! Administrators configure the OpenRouter API key, the model, and a monthly spend limit
//! per member ([`settings`]); every question's cost is recorded ([`record_usage`]) and a
//! member past the limit is refused until the next calendar month (UTC). Administrators
//! are not limited. [`openrouter`]
//! talks to OpenRouter and [`ruling`] holds the prompt, the tools, and the question flow.

pub mod openrouter;
pub mod ruling;

use serde::Serialize;
use time::{Date, Month, Time};

use crate::accounts::user::{decrypt_secret, encrypt_secret};
use crate::db::{self, IsoDate, Pool, UtcDateTime};
use crate::patch::Patch;

use self::openrouter::Usage;

/// The configured OpenRouter credentials and limit. `Debug` redacts the key.
#[derive(Clone, Default, PartialEq)]
pub struct AiSettings {
    /// The OpenRouter API key, decrypted.
    pub api_key: Option<String>,
    /// The OpenRouter model id, such as `anthropic/claude-sonnet-5.5`.
    pub model: Option<String>,
    /// Most each member may spend per calendar month, in USD; `None` for no limit.
    pub monthly_limit_usd: Option<f64>,
}

impl std::fmt::Debug for AiSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiSettings")
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("model", &self.model)
            .field("monthly_limit_usd", &self.monthly_limit_usd)
            .finish()
    }
}

impl AiSettings {
    /// The key and model, when both are set.
    pub fn credentials(&self) -> Option<(&str, &str)> {
        Some((self.api_key.as_deref()?, self.model.as_deref()?))
    }
}

/// The stored settings; a key that no longer decrypts (the server secret changed) reads as
/// unset.
pub async fn settings(pool: &Pool, secret_key: &str) -> Result<AiSettings, sqlx::Error> {
    let row =
        sqlx::query!("SELECT api_key, model, monthly_limit_usd FROM ai_settings WHERE id = 1")
            .fetch_optional(pool)
            .await?;
    Ok(row.map_or_else(AiSettings::default, |row| AiSettings {
        api_key: row
            .api_key
            .and_then(|stored| decrypt_secret(secret_key, &stored)),
        model: row.model,
        monthly_limit_usd: row.monthly_limit_usd,
    }))
}

/// Stores the model and limit and, unless unchanged, the API key (sealed; `None` removes
/// it).
pub async fn save_settings(
    pool: &Pool,
    secret_key: &str,
    api_key: &Patch<String>,
    model: Option<&str>,
    monthly_limit_usd: Option<f64>,
) -> Result<(), sqlx::Error> {
    let keep_key = !api_key.is_set();
    let sealed = match api_key {
        Patch::Set(Some(key)) => Some(encrypt_secret(secret_key, key)),
        _ => None,
    };
    let now = UtcDateTime::now();
    let mut tx = db::begin(pool).await?;
    sqlx::query!(
        "INSERT INTO ai_settings (id, api_key, model, monthly_limit_usd, inserted_at, updated_at)
         VALUES (1, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET
            api_key = CASE WHEN ? THEN ai_settings.api_key ELSE excluded.api_key END,
            model = excluded.model, monthly_limit_usd = excluded.monthly_limit_usd,
            updated_at = excluded.updated_at",
        sealed,
        model,
        monthly_limit_usd,
        now,
        now,
        keep_key
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// The first day of `now`'s calendar month (UTC) and of the next one.
pub fn budget_month(now: UtcDateTime) -> (IsoDate, IsoDate) {
    let today = now.date();
    let start = today.replace_day(1).unwrap_or(today);
    let (year, month) = match today.month() {
        Month::December => (today.year().saturating_add(1), Month::January),
        month => (today.year(), month.next()),
    };
    let next = Date::from_calendar_date(year, month, 1).unwrap_or(start);
    (IsoDate(start), IsoDate(next))
}

fn month_start(now: UtcDateTime) -> UtcDateTime {
    let (start, _) = budget_month(now);
    UtcDateTime::from_offset(start.0.with_time(Time::MIDNIGHT).assume_utc())
}

/// What `user_id` has spent on rulings this calendar month (UTC), in USD.
pub async fn spent_this_month(pool: &Pool, user_id: i64) -> Result<f64, sqlx::Error> {
    let since = month_start(UtcDateTime::now());
    sqlx::query_scalar!(
        r#"SELECT coalesce(sum(cost_usd), 0.0) AS "spent!: f64" FROM ai_ruling_usage
           WHERE user_id = ? AND inserted_at >= ?"#,
        user_id,
        since
    )
    .fetch_one(pool)
    .await
}

/// Records one question's cost.
pub async fn record_usage(
    pool: &Pool,
    user_id: i64,
    model: &str,
    usage: &Usage,
) -> Result<(), sqlx::Error> {
    let now = UtcDateTime::now();
    let mut tx = db::begin(pool).await?;
    sqlx::query!(
        "INSERT INTO ai_ruling_usage (user_id, model, cost_usd, prompt_tokens, completion_tokens,
            cached_tokens, cache_write_tokens, inserted_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        user_id,
        model,
        usage.cost_usd,
        usage.prompt_tokens,
        usage.completion_tokens,
        usage.cached_tokens,
        usage.cache_write_tokens,
        now
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// One member's rulings this month.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemberUsage {
    /// The member.
    pub user_id: i64,
    /// Their display name.
    pub display_name: String,
    /// Questions asked.
    pub questions: i64,
    /// Spent, in USD.
    pub spent_usd: f64,
}

/// Every member who asked this calendar month (UTC), biggest spender first.
pub async fn usage_this_month(pool: &Pool) -> Result<Vec<MemberUsage>, sqlx::Error> {
    let since = month_start(UtcDateTime::now());
    sqlx::query_as!(
        MemberUsage,
        r#"SELECT users.id AS "user_id!", users.display_name,
                  count(*) AS "questions!: i64", coalesce(sum(cost_usd), 0.0) AS "spent_usd!: f64"
           FROM ai_ruling_usage JOIN users ON users.id = ai_ruling_usage.user_id
           WHERE ai_ruling_usage.inserted_at >= ?
           GROUP BY users.id ORDER BY 4 DESC, 2"#,
        since
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_run_by_calendar_month() {
        let at = |value: &str| UtcDateTime::parse(value).unwrap();
        let month = |value: &str| {
            let (start, next) = budget_month(at(value));
            (start.to_string(), next.to_string())
        };
        assert_eq!(
            month("2026-10-21T04:00:00Z"),
            ("2026-10-01".into(), "2026-11-01".into())
        );
        assert_eq!(
            month("2026-12-31T23:59:59Z"),
            ("2026-12-01".into(), "2027-01-01".into())
        );
        assert_eq!(
            month_start(at("2026-03-01T00:00:00Z")),
            at("2026-03-01T00:00:00Z")
        );
    }
}
