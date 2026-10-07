//! Finding the game a summary card shows (`TheGathering.Games.Summary`).

use std::sync::LazyLock;

use sqlx::SqliteConnection;

use crate::regex::{Regex, compile};

use super::GamesError;
use super::model::{Game, load_game};

static SPELLBOT: LazyLock<Regex> = LazyLock::new(|| compile(r"(?i)\A#?SB[0-9]{1,20}\z"));
static LOCAL_ID: LazyLock<Regex> = LazyLock::new(|| compile(r"\A[0-9]{1,18}\z"));

/// `Games.find_summary_game/1`: a local game id, an `SB`-prefixed SpellBot id (optionally
/// with `#`), or the latest game for a blank reference. Anything else is a bad request.
pub async fn find(conn: &mut SqliteConnection, reference: &str) -> Result<Game, GamesError> {
    let reference = reference.trim();
    let id = if reference.is_empty() {
        sqlx::query_scalar!(r#"SELECT id AS "id!: i64" FROM games ORDER BY played_at DESC, id DESC LIMIT 1"#)
            .fetch_optional(&mut *conn)
            .await?
    } else if SPELLBOT.is_match(reference) {
        let external_id = format!("spellbot:{}", reference.trim_start_matches('#').to_uppercase());
        sqlx::query_scalar!(
            r#"SELECT id AS "id!: i64" FROM games WHERE source = 'discord' AND external_id = ?"#,
            external_id
        )
        .fetch_optional(&mut *conn)
        .await?
    } else if LOCAL_ID.is_match(reference) {
        let id: i64 = reference.parse().map_err(|_| GamesError::BadRequest)?;
        Some(id)
    } else {
        return Err(GamesError::BadRequest);
    };
    match id {
        Some(id) => load_game(conn, id).await?.ok_or(GamesError::NotFound),
        None => Err(GamesError::NotFound),
    }
}
