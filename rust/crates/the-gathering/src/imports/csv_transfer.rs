//! CSV imports that create, update, or skip games.
//!
//! The preview runs the whole import in a transaction and rolls it back, so it reports
//! exactly what the commit would do, plus a revision fingerprint of the file and the
//! affected tables. Updates only commit against an unchanged revision.

use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqliteConnection;

use crate::db;
use crate::games::DeckLinks;
use crate::games::{Game, deck, fold_name, load_game, model, player, record_game};
use crate::state::AppState;

use super::commit::{self, SeatAttrs};
use super::csv::hex;
use super::csv_changes::{self, Change, GameView, SeatView};
use super::preview::{self, Preview, Source};
use super::{ImportError, ImportGame, ImportResult, ImportSeat, LineError};

/// What the import does to one game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Review {
    /// The file's game id.
    pub game_id: String,
    /// The existing or created game.
    pub target_id: Option<i64>,
    /// `create`, `update`, or `skip`.
    pub action: &'static str,
    /// Material changes (updates only).
    pub changes: Vec<Change>,
}

fn review(
    game: &ImportGame,
    target: Option<i64>,
    action: &'static str,
    changes: Vec<Change>,
) -> Review {
    Review {
        game_id: game.game_id.clone(),
        target_id: target,
        action,
        changes,
    }
}

fn message(error: &ImportError) -> String {
    match error {
        ImportError::Message(message) => message.clone(),
        ImportError::Invalid(errors) => errors.to_string(),
        ImportError::Validation(_) => "invalid".to_owned(),
        ImportError::Database(error) => error.to_string(),
    }
}

fn invalid(mut base: Preview, error: &ImportError) -> Preview {
    base.valid = false;
    base.errors
        .push(LineError::new(1, "import", message(error)));
    base
}

/// `CSVTransfer.preview/1`: the parse preview plus, when valid, a dry run's review and
/// the revision.
pub async fn preview(state: &AppState, csv: &str) -> Result<Preview, sqlx::Error> {
    let base = {
        let mut conn = state.pool.acquire().await?;
        preview::run(&mut conn, Source::Csv, csv).await?
    };
    if !base.valid {
        return Ok(base);
    }
    let mut tx = db::begin(&state.pool).await?;
    let revision = revision(&mut tx, csv).await?;
    let result = execute(&mut tx, state.games.deck_links(), &base.games, None).await;
    tx.rollback().await?;
    match result {
        Ok(review) => Ok(Preview {
            revision: Some(revision),
            review: Some(review),
            ..base
        }),
        Err(ImportError::Database(error)) => Err(error),
        Err(error) => Ok(invalid(base, &error)),
    }
}

/// `CSVTransfer.run/3`.
pub async fn run(
    state: &AppState,
    csv: &str,
    user_id: Option<i64>,
    reviewed_revision: Option<&str>,
) -> Result<ImportResult, ImportError> {
    let base = {
        let mut conn = state.pool.acquire().await?;
        preview::run(&mut conn, Source::Csv, csv).await?
    };
    if !base.valid {
        return Err(ImportError::Validation(Box::new(base)));
    }
    let mut tx = db::begin(&state.pool).await?;
    let result = async {
        if base
            .games
            .iter()
            .any(|game| game.action.as_deref() == Some("update"))
            && reviewed_revision != Some(revision(&mut tx, csv).await?.as_str())
        {
            return Err(ImportError::Message(
                "Preview is stale or missing. Preview again before updating games.".to_owned(),
            ));
        }
        execute(&mut tx, state.games.deck_links(), &base.games, user_id).await
    }
    .await;
    let reviews = match result {
        Ok(reviews) => {
            tx.commit().await?;
            reviews
        }
        Err(ImportError::Database(error)) => return Err(error.into()),
        Err(error) => {
            tx.rollback().await?;
            return Err(ImportError::Validation(Box::new(invalid(base, &error))));
        }
    };
    for reviewed in reviews.iter().filter(|reviewed| reviewed.action != "skip") {
        if let Some(id) = reviewed.target_id {
            state.games.link_catalog_cards(id).await?;
        }
    }
    let count = |action: &str| {
        i64::try_from(
            reviews
                .iter()
                .filter(|reviewed| reviewed.action == action)
                .count(),
        )
        .unwrap_or(i64::MAX)
    };
    Ok(ImportResult {
        created: count("create"),
        updated: Some(count("update")),
        skipped: count("skip"),
        game_ids: reviews
            .iter()
            .filter_map(|reviewed| reviewed.target_id)
            .collect(),
    })
}

async fn execute(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    games: &[ImportGame],
    user_id: Option<i64>,
) -> Result<Vec<Review>, ImportError> {
    let mut targets = Vec::with_capacity(games.len());
    for game in games {
        targets.push(target(conn, game).await?);
    }
    let mut ids: Vec<i64> = games
        .iter()
        .zip(&targets)
        .filter(|(game, _)| game.action.as_deref() != Some("skip"))
        .filter_map(|(_, target)| target.as_ref().map(|target| target.id))
        .collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != count {
        return Err(ImportError::Message(
            "Multiple CSV games target the same existing game.".to_owned(),
        ));
    }
    let mut reviews = Vec::with_capacity(games.len());
    for (game, target) in games.iter().zip(targets) {
        reviews.push(transfer(conn, links, game, target, user_id).await?);
    }
    Ok(reviews)
}

async fn target(
    conn: &mut SqliteConnection,
    game: &ImportGame,
) -> Result<Option<Game>, ImportError> {
    if game.action.as_deref() == Some("skip") {
        return Ok(None);
    }
    let portable = match &game.target_portable_id {
        Some(portable_id) => {
            sqlx::query_scalar!(
                r#"SELECT id AS "id!: i64" FROM games WHERE portable_id = ?"#,
                portable_id
            )
            .fetch_optional(&mut *conn)
            .await?
        }
        None => None,
    };
    let source = game.target_source.as_deref().unwrap_or("csv");
    let external_id = game
        .target_external_id
        .as_deref()
        .unwrap_or(&game.external_id);
    let external = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM games WHERE source = ? AND external_id = ?"#,
        source,
        external_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    let fail = |message: String| Err(ImportError::Message(message));
    if let (Some(portable), Some(external)) = (portable, external)
        && portable != external
    {
        return fail("Game identities refer to different existing games.".to_owned());
    }
    let update = game.action.as_deref() == Some("update");
    if update && game.target_external_id.is_some() && external.is_none() {
        return fail(format!(
            "Game {}: source/external_id was not found.",
            game.game_id
        ));
    }
    if game.target_portable_id.is_some() && portable.is_none() {
        return fail(format!("Game {}: portable ID was not found.", game.game_id));
    }
    let Some(found) = portable.or(external) else {
        if update {
            return fail(format!(
                "Game {}: update target was not found; no game will be created.",
                game.game_id
            ));
        }
        return Ok(None);
    };
    Ok(load_game(conn, found).await?)
}

async fn transfer(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    game: &ImportGame,
    target: Option<Game>,
    user_id: Option<i64>,
) -> Result<Review, ImportError> {
    let action = game.action.as_deref().unwrap_or("create");
    if action == "skip" {
        return Ok(review(game, None, "skip", Vec::new()));
    }
    if action == "create"
        && let Some(target) = &target
    {
        return Ok(review(game, Some(target.id), "skip", Vec::new()));
    }
    let mut seats = Vec::with_capacity(game.seats.len());
    for seat in &game.seats {
        seats.push(seat_attrs(conn, links, seat).await?);
    }
    let mut attrs = Map::new();
    attrs.insert("played_at".into(), json!(game.played_at));
    for (key, value) in [
        ("duration_minutes", json!(game.duration_minutes)),
        ("turns", json!(game.turns)),
        ("win_condition", json!(game.win_condition)),
        ("notes", json!(game.notes)),
    ] {
        if !value.is_null() {
            attrs.insert(key.into(), value);
        }
    }
    match target {
        Some(target) => {
            let merged = update_seats(&seats, &target);
            let projected = project(conn, &target, &attrs, &merged).await?;
            let changes = csv_changes::diff(&GameView::of(&target), &projected);
            if changes.is_empty() {
                return Ok(review(game, Some(target.id), "skip", Vec::new()));
            }
            prepare_seats(conn, &target, &seats).await?;
            let current = load_game(conn, target.id)
                .await?
                .ok_or(ImportError::Database(sqlx::Error::RowNotFound))?;
            attrs.insert(
                "seats".into(),
                Value::Array(merged.into_iter().map(Value::Object).collect()),
            );
            let saved = record_game::update(conn, &current, &Value::Object(attrs)).await?;
            Ok(review(game, Some(saved.id), "update", changes))
        }
        None => {
            attrs.insert(
                "seats".into(),
                Value::Array(seats.iter().map(SeatAttrs::to_json).collect()),
            );
            attrs.insert(
                "source".into(),
                json!(game.target_source.as_deref().unwrap_or("csv")),
            );
            attrs.insert(
                "external_id".into(),
                json!(
                    game.target_external_id
                        .as_deref()
                        .unwrap_or(&game.external_id)
                ),
            );
            let saved = record_game::create(conn, &Value::Object(attrs), user_id).await?;
            Ok(review(game, Some(saved.id), "create", Vec::new()))
        }
    }
}

/// Reuses decks by commander pair when possible, but never rewrites a shared deck's
/// commander.
async fn seat_attrs(
    conn: &mut SqliteConnection,
    links: &DeckLinks,
    seat: &ImportSeat,
) -> Result<SeatAttrs, ImportError> {
    if let Some(found) = player::find_player_by_name(conn, &seat.player).await?
        && let Some(existing) = deck::find_deck(conn, found.id, &seat.deck, None, None).await?
        && pair(
            Some(&existing.commander_name),
            existing.partner_name.as_deref(),
        ) != pair(Some(&seat.commander), seat.partner_name.as_deref())
    {
        return Err(ImportError::Message(format!(
            "{}: deck {} has different commanders. Use a different deck name.",
            seat.player, seat.deck
        )));
    }
    commit::commit_seat(conn, links, seat).await
}

fn pair(commander: Option<&str>, partner: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = [commander, partner]
        .into_iter()
        .flatten()
        .map(fold_name)
        .collect();
    names.sort();
    names
}

/// The `seats` params for updating `target`: retained players keep their row (and every
/// field the file leaves blank); an eliminator who left the game is cleared.
fn update_seats(seats: &[SeatAttrs], target: &Game) -> Vec<Map<String, Value>> {
    let ids: Vec<i64> = seats.iter().map(|seat| seat.player_id).collect();
    seats
        .iter()
        .map(|seat| {
            let Value::Object(mut params) = seat.to_json() else {
                return Map::new();
            };
            if let Some(existing) = target.seats.iter().find(|s| s.player_id == seat.player_id) {
                params.retain(|_, value| !value.is_null());
                params.insert("id".into(), json!(existing.id));
                if existing
                    .eliminated_by_player_id
                    .is_some_and(|eliminator| !ids.contains(&eliminator))
                {
                    params.insert("eliminated_by_player_id".into(), Value::Null);
                }
            }
            params
        })
        .collect()
}

/// The param when given (a cast change), else the current value.
fn int_or(params: &Map<String, Value>, key: &str, current: Option<i64>) -> Option<i64> {
    match params.get(key) {
        Some(value) => value.as_i64(),
        None => current,
    }
}

/// The param when given (a cast change), else the current value.
fn str_or(params: &Map<String, Value>, key: &str, current: Option<String>) -> Option<String> {
    match params.get(key) {
        Some(value) => value.as_str().map(str::to_owned),
        None => current,
    }
}

/// `target` with `attrs` and `seats` applied, as `Ecto.Changeset.apply_action/2` would
/// build it, with each seat's player and deck loaded.
async fn project(
    conn: &mut SqliteConnection,
    target: &Game,
    attrs: &Map<String, Value>,
    seats: &[Map<String, Value>],
) -> Result<GameView, ImportError> {
    let current = GameView::of(target);
    let mut view = GameView {
        played_at: attrs
            .get("played_at")
            .and_then(Value::as_str)
            .and_then(db::UtcDateTime::parse)
            .unwrap_or(current.played_at),
        duration_minutes: int_or(attrs, "duration_minutes", current.duration_minutes),
        turns: int_or(attrs, "turns", current.turns),
        win_condition: str_or(attrs, "win_condition", current.win_condition),
        notes: str_or(attrs, "notes", current.notes),
        seats: Vec::with_capacity(seats.len()),
    };
    for params in seats {
        let existing = params
            .get("id")
            .and_then(Value::as_i64)
            .and_then(|id| target.seats.iter().find(|seat| seat.id == id));
        let player_id =
            int_or(params, "player_id", existing.map(|seat| seat.player_id)).unwrap_or_default();
        let deck_id = int_or(params, "deck_id", existing.and_then(|seat| seat.deck_id));
        let player_name = model::get_player(conn, player_id)
            .await?
            .map(|player| player.name)
            .unwrap_or_default();
        let deck = match deck_id {
            Some(id) => model::get_deck(conn, id).await?,
            None => None,
        };
        view.seats.push(SeatView {
            player_id,
            player_name,
            deck,
            seat: int_or(params, "seat", existing.map(|seat| seat.seat)).unwrap_or_default(),
            result: str_or(
                params,
                "result",
                existing.map(|seat| seat.result.as_str().to_owned()),
            )
            .unwrap_or_default(),
            kills: int_or(params, "kills", existing.and_then(|seat| seat.kills)),
            mvp_card_name: str_or(
                params,
                "mvp_card_name",
                existing.and_then(|seat| seat.mvp_card_name.clone()),
            ),
            eliminated_by_player_id: int_or(
                params,
                "eliminated_by_player_id",
                existing.and_then(|seat| seat.eliminated_by_player_id),
            ),
        });
    }
    Ok(view)
}

/// Avoids unique-index collisions while swapping seats. The enclosing transaction rolls
/// this back along with all other writes if any validation fails.
async fn prepare_seats(
    conn: &mut SqliteConnection,
    target: &Game,
    seats: &[SeatAttrs],
) -> Result<(), sqlx::Error> {
    let ids: Vec<i64> = seats.iter().map(|seat| seat.player_id).collect();
    let ids_json = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into());
    sqlx::query!(
        "DELETE FROM game_players WHERE game_id = ? AND player_id NOT IN (SELECT value FROM json_each(?))",
        target.id,
        ids_json
    )
    .execute(&mut *conn)
    .await?;
    let reordered = seats.iter().any(|seat| {
        target
            .seats
            .iter()
            .any(|held| held.player_id == seat.player_id && held.seat != seat.seat)
    });
    if reordered {
        sqlx::query!(
            "UPDATE game_players SET seat = seat + 10 WHERE game_id = ?",
            target.id
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// A fingerprint of the file and every game, seat, player, and deck row.
async fn revision(conn: &mut SqliteConnection, csv: &str) -> Result<String, sqlx::Error> {
    let snapshot = sqlx::query!(
        r#"SELECT
             (SELECT json_group_array(json_array(id, played_at, duration_minutes, turns, notes, source, external_id,
                                                 created_by_user_id, inserted_at, updated_at, portable_id,
                                                 win_condition, format))
              FROM (SELECT * FROM games ORDER BY id)) AS "games!: String",
             (SELECT json_group_array(json_array(id, game_id, player_id, deck_id, seat, result, eliminated_turn,
                                                 eliminated_by_player_id, mvp_card_id, mvp_card_name, notes,
                                                 inserted_at, updated_at, kills))
              FROM (SELECT * FROM game_players ORDER BY id)) AS "seats!: String",
             (SELECT json_group_array(json_array(id, name, user_id, discord_id, archived_at, inserted_at, updated_at))
              FROM (SELECT * FROM players ORDER BY id)) AS "players!: String",
             (SELECT json_group_array(json_array(id, player_id, name, commander_card_id, commander_name,
                                                 partner_card_id, partner_name, color_identity, decklist_url,
                                                 decklist_source, archived_at, inserted_at, updated_at, skip_count,
                                                 included_for_play, commander_printing_id, partner_printing_id))
              FROM (SELECT * FROM decks ORDER BY id)) AS "decks!: String""#
    )
    .fetch_one(&mut *conn)
    .await?;
    let mut hasher = Sha256::new();
    for part in [
        csv,
        &snapshot.games,
        &snapshot.seats,
        &snapshot.players,
        &snapshot.decks,
    ] {
        hasher.update(part.len().to_be_bytes());
        hasher.update(part.as_bytes());
    }
    Ok(hex(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::ValidationError;

    #[test]
    fn validation_failures_read_as_sentences() {
        let mut errors = ValidationError::single("played_at", "can't be blank");
        errors.set_rows(
            "seats",
            vec![
                ValidationError::new(),
                ValidationError::single("kills", "must be greater than or equal to 0"),
            ],
        );
        assert_eq!(
            message(&ImportError::Invalid(errors)),
            "Played at can't be blank; Seat 2: Kills must be greater than or equal to 0"
        );
    }
}
