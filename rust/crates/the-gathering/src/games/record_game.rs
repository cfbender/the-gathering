//! Recording and editing games.
//!
//! Every function runs on a connection and opens a savepoint (or transaction) of its own,
//! so it can run inside a caller's transaction (imports) or on its own.

use serde_json::{Map, Value};
use sqlx::{Connection, SqliteConnection};

use crate::db::{self, UtcDateTime};

use super::GamesError;
use super::game::{Extra, ValidGame, ValidSeat, changeset};
use super::model::{Game, load_game};

fn attr_str<'a>(attrs: &'a Value, key: &str) -> Option<&'a str> {
    attrs.get(key).and_then(Value::as_str)
}

async fn game_by_external_id(
    conn: &mut SqliteConnection,
    source: &str,
    external_id: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM games WHERE source = ? AND external_id = ?"#,
        source,
        external_id
    )
    .fetch_optional(&mut *conn)
    .await
}

async fn reload(conn: &mut SqliteConnection, id: i64) -> Result<Game, GamesError> {
    load_game(conn, id).await?.ok_or(GamesError::NotFound)
}

/// `RecordGame.create/2`: with a non-empty `external_id`, an existing game from the same
/// `source` (default `manual`) is returned unchanged instead of recording a duplicate.
pub async fn create(
    conn: &mut SqliteConnection,
    attrs: &Value,
    created_by_user_id: Option<i64>,
) -> Result<Game, GamesError> {
    let source = match attrs.get("source") {
        None => Some("manual"),
        Some(value) => value.as_str(),
    };
    match (
        source,
        attr_str(attrs, "external_id").filter(|id| !id.is_empty()),
    ) {
        (Some(source), Some(external_id)) => {
            match game_by_external_id(conn, source, external_id).await? {
                Some(id) => reload(conn, id).await,
                None => insert(conn, attrs, created_by_user_id, Some((source, external_id))).await,
            }
        }
        _ => insert(conn, attrs, created_by_user_id, None).await,
    }
}

fn with_identity(attrs: &Value, source: &str, external_id: &str) -> Value {
    let mut merged: Map<String, Value> = attrs.as_object().cloned().unwrap_or_default();
    merged.insert("source".into(), Value::String(source.to_owned()));
    merged.insert("external_id".into(), Value::String(external_id.to_owned()));
    Value::Object(merged)
}

/// `RecordGame.find_or_create_by_external_id/3`.
pub async fn find_or_create_by_external_id(
    conn: &mut SqliteConnection,
    source: &str,
    external_id: &str,
    attrs: &Value,
) -> Result<Game, GamesError> {
    create(conn, &with_identity(attrs, source, external_id), None).await
}

/// `RecordGame.upsert_by_external_id/3`: updates the game with this identity, or records it.
pub async fn upsert_by_external_id(
    conn: &mut SqliteConnection,
    source: &str,
    external_id: &str,
    attrs: &Value,
) -> Result<Game, GamesError> {
    let attrs = with_identity(attrs, source, external_id);
    match game_by_external_id(conn, source, external_id).await? {
        Some(id) => {
            let game = reload(conn, id).await?;
            update(conn, &game, &attrs).await
        }
        None => insert(conn, &attrs, None, Some((source, external_id))).await,
    }
}

/// Inserts a validated game.
///
/// Elixir recovered from a concurrent insert of the same `(source, external_id)` by looking
/// for an `external_id` error, but `unique_constraint([:source, :external_id])` reports on
/// `source`, so the recovery never ran; this returns the existing game as intended.
async fn insert(
    conn: &mut SqliteConnection,
    attrs: &Value,
    created_by_user_id: Option<i64>,
    identity: Option<(&str, &str)>,
) -> Result<Game, GamesError> {
    let game = changeset(
        conn,
        None,
        attrs,
        Extra {
            created_by_user_id,
            identity,
            portable: None,
        },
    )
    .await?;
    let mut tx = conn.begin().await?;
    match insert_rows(&mut tx, &game).await {
        Ok(id) => {
            tx.commit().await?;
            reload(conn, id).await
        }
        Err(error) if db::is_unique_violation(&error, &["games.source", "games.external_id"]) => {
            tx.rollback().await?;
            let (source, external_id) = identity.ok_or(GamesError::Database(error))?;
            let id = game_by_external_id(conn, source, external_id)
                .await?
                .ok_or(GamesError::NotFound)?;
            reload(conn, id).await
        }
        Err(error) => Err(error.into()),
    }
}

/// Validates a game a portable import would insert (`Game.changeset/2` with the export's
/// `portable_id`, `source`, and `external_id`, plus `put_created_by/2`), without writing.
pub async fn validate_portable(
    conn: &mut SqliteConnection,
    attrs: &Value,
    created_by_user_id: Option<i64>,
    identity: (&str, &str, Option<&str>),
) -> Result<(), GamesError> {
    changeset(
        conn,
        None,
        attrs,
        Extra {
            created_by_user_id,
            identity: None,
            portable: Some(identity),
        },
    )
    .await
    .map(|_| ())
}

/// Inserts a game from a portable export, keeping its `portable_id` and source identity
/// (`PortableImport` inserts with `Repo.insert/1`, not `RecordGame`). Unique violations
/// are validation errors, as `unique_constraint/2` reports them.
pub async fn insert_portable(
    conn: &mut SqliteConnection,
    attrs: &Value,
    created_by_user_id: Option<i64>,
    identity: (&str, &str, Option<&str>),
) -> Result<Game, GamesError> {
    let game = changeset(
        conn,
        None,
        attrs,
        Extra {
            created_by_user_id,
            identity: None,
            portable: Some(identity),
        },
    )
    .await?;
    let mut tx = conn.begin().await?;
    match insert_rows(&mut tx, &game).await {
        Ok(id) => {
            tx.commit().await?;
            reload(conn, id).await
        }
        Err(error) => {
            tx.rollback().await?;
            if db::is_unique_violation(&error, &["games.portable_id"]) {
                Err(crate::error::Errors::single("portable_id", crate::changeset::TAKEN).into())
            } else if db::is_unique_violation(&error, &["games.source", "games.external_id"]) {
                Err(crate::error::Errors::single("source", crate::changeset::TAKEN).into())
            } else {
                Err(error.into())
            }
        }
    }
}

async fn insert_rows(conn: &mut SqliteConnection, game: &ValidGame) -> Result<i64, sqlx::Error> {
    let now = UtcDateTime::now();
    let portable_id = game
        .portable_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let id = sqlx::query_scalar!(
        r#"INSERT INTO games (played_at, duration_minutes, turns, win_condition, notes, source, format, external_id,
                              portable_id, created_by_user_id, inserted_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id AS "id!: i64""#,
        game.played_at,
        game.duration_minutes,
        game.turns,
        game.win_condition,
        game.notes,
        game.source,
        game.format,
        game.external_id,
        portable_id,
        game.created_by_user_id,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await?;
    for seat in &game.seats {
        insert_seat(conn, id, seat, now).await?;
    }
    Ok(id)
}

async fn insert_seat(
    conn: &mut SqliteConnection,
    game_id: i64,
    seat: &ValidSeat,
    now: UtcDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO game_players (game_id, player_id, deck_id, seat, result, kills, eliminated_turn,
                                   eliminated_by_player_id, mvp_card_id, mvp_card_name, notes, inserted_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        game_id,
        seat.player_id,
        seat.deck_id,
        seat.seat,
        seat.result,
        seat.kills,
        seat.eliminated_turn,
        seat.eliminated_by_player_id,
        seat.mvp_card_id,
        seat.mvp_card_name,
        seat.notes,
        now,
        now
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Whether casting left a stored seat as it was.
fn unchanged(held: &super::model::Seat, seat: &ValidSeat) -> bool {
    held.player_id == seat.player_id
        && held.deck_id == seat.deck_id
        && held.seat == seat.seat
        && held.result == seat.result
        && held.kills == seat.kills
        && held.eliminated_turn == seat.eliminated_turn
        && held.eliminated_by_player_id == seat.eliminated_by_player_id
        && held.mvp_card_id == seat.mvp_card_id
        && held.mvp_card_name == seat.mvp_card_name
        && held.notes == seat.notes
}

/// Whether a requested seat number is held by another existing row (`seats_collide?/2`);
/// Ecto updated rows one at a time, so swapping numbers needs the rows parked first.
fn seats_collide(current: &Game, requested: &[ValidSeat]) -> bool {
    requested.iter().any(|seat| {
        current
            .seats
            .iter()
            .any(|held| held.seat == seat.seat && Some(held.id) != seat.existing_id)
    })
}

/// `RecordGame.update/2`: `played_at`, `duration_minutes`, `turns`, `win_condition`,
/// `format`, `notes`, and `seats` (matched to existing rows by `id`; rows left out are
/// deleted). Provenance (`source`, `external_id`, creator) never changes. All or nothing.
pub async fn update(
    conn: &mut SqliteConnection,
    game: &Game,
    attrs: &Value,
) -> Result<Game, GamesError> {
    let valid = changeset(conn, Some(game), attrs, Extra::default()).await?;
    let mut tx = conn.begin().await?;
    let now = UtcDateTime::now();
    let header_changed = valid.played_at != game.played_at
        || valid.duration_minutes != game.duration_minutes
        || valid.turns != game.turns
        || valid.win_condition != game.win_condition
        || valid.notes != game.notes
        || valid.format != game.format;
    if header_changed {
        sqlx::query!(
            "UPDATE games SET played_at = ?, duration_minutes = ?, turns = ?, win_condition = ?, notes = ?, format = ?,
                              updated_at = ?
             WHERE id = ?",
            valid.played_at,
            valid.duration_minutes,
            valid.turns,
            valid.win_condition,
            valid.notes,
            valid.format,
            now,
            game.id
        )
        .execute(&mut *tx)
        .await?;
    }
    if valid.seats_given {
        let kept: Vec<i64> = valid
            .seats
            .iter()
            .filter_map(|seat| seat.existing_id)
            .collect();
        for removed in game.seats.iter().filter(|seat| !kept.contains(&seat.id)) {
            sqlx::query!("DELETE FROM game_players WHERE id = ?", removed.id)
                .execute(&mut *tx)
                .await?;
        }
        let parked = seats_collide(game, &valid.seats);
        if parked {
            sqlx::query!(
                "UPDATE game_players SET seat = -seat WHERE game_id = ?",
                game.id
            )
            .execute(&mut *tx)
            .await?;
        }
        for seat in &valid.seats {
            match seat.existing_id {
                // Like Ecto, a seat whose fields did not change is not written, so it keeps
                // its `updated_at` (CSV corrections rely on this).
                Some(id)
                    if !parked
                        && game
                            .seats
                            .iter()
                            .any(|held| held.id == id && unchanged(held, seat)) => {}
                Some(id) => {
                    sqlx::query!(
                        "UPDATE game_players SET player_id = ?, deck_id = ?, seat = ?, result = ?, kills = ?,
                                eliminated_turn = ?, eliminated_by_player_id = ?, mvp_card_id = ?, mvp_card_name = ?,
                                notes = ?, updated_at = ?
                         WHERE id = ?",
                        seat.player_id,
                        seat.deck_id,
                        seat.seat,
                        seat.result,
                        seat.kills,
                        seat.eliminated_turn,
                        seat.eliminated_by_player_id,
                        seat.mvp_card_id,
                        seat.mvp_card_name,
                        seat.notes,
                        now,
                        id
                    )
                    .execute(&mut *tx)
                    .await?;
                }
                None => insert_seat(&mut tx, game.id, seat, now).await?,
            }
        }
    }
    tx.commit().await?;
    reload(conn, game.id).await
}

/// `Games.delete_game/1` (seats cascade).
pub async fn delete(conn: &mut SqliteConnection, game_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM games WHERE id = ?", game_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}
