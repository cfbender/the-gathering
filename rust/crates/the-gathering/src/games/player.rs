//! Players: listing, detail, identity administration, and `Player.changeset/2`.

use serde_json::Value;
use sqlx::SqliteConnection;

use crate::changeset::{Changeset, trim};
use crate::db::{self, UtcDateTime};
use crate::validation::TAKEN;
use crate::validation::ValidationError;

use super::model::{Deck, GameFormat, GameResult, Player, select_decks, select_players};
use super::{GamesError, Pagination, fold_name, user_exists};

/// A player with their decks (by name) and seats (newest game first) for the player page
/// (`Games.get_player!/1`).
#[derive(Clone, Debug)]
pub struct PlayerDetail {
    /// The player, with the linked user's avatar.
    pub player: Player,
    /// Every deck, archived included, ordered by name.
    pub decks: Vec<Deck>,
    /// Every seat, newest game first.
    pub seats: Vec<SeatGame>,
}

/// A seat with its game's header and deck (`GameJSON.seat_game/2`).
#[derive(Clone, Debug)]
pub struct SeatGame {
    /// The game.
    pub game_id: i64,
    /// When it was played.
    pub played_at: UtcDateTime,
    /// The game's format.
    pub format: GameFormat,
    /// The seat's result.
    pub result: GameResult,
    /// The seat's deck, when loaded and present.
    pub deck: Option<Deck>,
}

/// A row of the administrator's identity list (`Games.list_player_identities/1`).
#[derive(Clone, Debug)]
pub struct PlayerIdentityRow {
    /// The player.
    pub player: Player,
    /// The linked account's id and username.
    pub user: Option<(i64, String)>,
}

/// `Games.list_players/1`, ordered by case-folded name, with avatars.
pub async fn list_players(
    conn: &mut SqliteConnection,
    include_archived: bool,
) -> Result<Vec<Player>, sqlx::Error> {
    sqlx::query_as!(
        Player,
        r#"SELECT p.id AS "id!", p.name, p.user_id, p.discord_id, p.archived_at AS "archived_at: UtcDateTime",
                  u.avatar_url AS "avatar_url?: String",
                  p.inserted_at AS "inserted_at: UtcDateTime", p.updated_at AS "updated_at: UtcDateTime"
           FROM players p LEFT JOIN users u ON u.id = p.user_id
           WHERE ? OR p.archived_at IS NULL
           ORDER BY lower(p.name)"#,
        include_archived
    )
    .fetch_all(&mut *conn)
    .await
}

/// A player with the linked user's avatar.
pub async fn get_player_with_avatar(
    conn: &mut SqliteConnection,
    id: i64,
) -> Result<Option<Player>, sqlx::Error> {
    sqlx::query_as!(
        Player,
        r#"SELECT p.id AS "id!", p.name, p.user_id, p.discord_id, p.archived_at AS "archived_at: UtcDateTime",
                  u.avatar_url AS "avatar_url?: String",
                  p.inserted_at AS "inserted_at: UtcDateTime", p.updated_at AS "updated_at: UtcDateTime"
           FROM players p LEFT JOIN users u ON u.id = p.user_id
           WHERE p.id = ?"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await
}

/// `Games.get_player!/1` (without the raise): decks and seats preloaded.
pub async fn get_player_detail(
    conn: &mut SqliteConnection,
    id: i64,
) -> Result<Option<PlayerDetail>, sqlx::Error> {
    let Some(player) = get_player_with_avatar(conn, id).await? else {
        return Ok(None);
    };
    let decks = select_decks!("WHERE player_id = ? ORDER BY name", id)
        .fetch_all(&mut *conn)
        .await?;
    let rows = sqlx::query!(
        r#"SELECT s.game_id, s.deck_id, s.result AS "result: GameResult", g.played_at AS "played_at: UtcDateTime",
                  g.format AS "format: GameFormat"
           FROM game_players s JOIN games g ON g.id = s.game_id
           WHERE s.player_id = ?
           ORDER BY g.played_at DESC"#,
        id
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut seats = Vec::with_capacity(rows.len());
    for row in rows {
        let deck = match row.deck_id {
            Some(deck_id) => match decks.iter().find(|deck| deck.id == deck_id) {
                Some(deck) => Some(deck.clone()),
                None => super::model::get_deck(conn, deck_id).await?,
            },
            None => None,
        };
        seats.push(SeatGame {
            game_id: row.game_id,
            played_at: row.played_at,
            format: row.format,
            result: row.result,
            deck,
        });
    }
    Ok(Some(PlayerDetail {
        player,
        decks,
        seats,
    }))
}

/// `Games.get_player_for_user/1`.
pub async fn get_player_for_user(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<Option<Player>, sqlx::Error> {
    select_players!("WHERE user_id = ?", user_id)
        .fetch_optional(&mut *conn)
        .await
}

fn positive(value: Option<&Value>, default: i64) -> i64 {
    match value {
        Some(Value::Number(number)) => number
            .as_i64()
            .filter(|value| *value > 0)
            .unwrap_or(default),
        Some(Value::String(text)) => text
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(default),
        _ => default,
    }
}

/// `Games.list_player_identities/1`: search by name, Discord id, or username; paginated.
pub async fn list_player_identities(
    conn: &mut SqliteConnection,
    params: &Value,
) -> Result<(Vec<PlayerIdentityRow>, Pagination), sqlx::Error> {
    let page = positive(params.get("page"), 1);
    let per_page = positive(params.get("per_page"), 50).min(100);
    let search = params
        .get("search")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!: i64" FROM players p LEFT JOIN users u ON u.id = p.user_id
           WHERE instr(lower(p.name), ?1) > 0 OR instr(p.discord_id, ?1) > 0 OR instr(lower(u.username), ?1) > 0"#,
        search
    )
    .fetch_one(&mut *conn)
    .await?;
    let offset = page.saturating_sub(1).saturating_mul(per_page);
    let rows = sqlx::query!(
        r#"SELECT p.id AS "id!", p.name, p.user_id, p.discord_id, p.archived_at AS "archived_at: UtcDateTime",
                  p.inserted_at AS "inserted_at: UtcDateTime", p.updated_at AS "updated_at: UtcDateTime",
                  u.id AS "linked_id?: i64", u.username AS "username?: String"
           FROM players p LEFT JOIN users u ON u.id = p.user_id
           WHERE instr(lower(p.name), ?1) > 0 OR instr(p.discord_id, ?1) > 0 OR instr(lower(u.username), ?1) > 0
           ORDER BY lower(p.name), p.id
           LIMIT ?2 OFFSET ?3"#,
        search,
        per_page,
        offset
    )
    .fetch_all(&mut *conn)
    .await?;
    let players = rows
        .into_iter()
        .map(|row| PlayerIdentityRow {
            player: Player {
                id: row.id,
                name: row.name,
                user_id: row.user_id,
                discord_id: row.discord_id,
                archived_at: row.archived_at,
                avatar_url: None,
                inserted_at: row.inserted_at,
                updated_at: row.updated_at,
            },
            user: row.linked_id.zip(row.username),
        })
        .collect();
    Ok((players, Pagination::new(page, per_page, total)))
}

/// `Games.unlink_player_identity/1`: detaches the account and Discord identity.
pub async fn unlink_player_identity(
    conn: &mut SqliteConnection,
    player: &Player,
) -> Result<Player, sqlx::Error> {
    let now = UtcDateTime::now();
    sqlx::query!(
        "UPDATE players SET discord_id = NULL, user_id = NULL, updated_at = ? WHERE id = ?",
        now,
        player.id
    )
    .execute(&mut *conn)
    .await?;
    Ok(Player {
        discord_id: None,
        user_id: None,
        updated_at: now,
        ..player.clone()
    })
}

fn unique_error(error: sqlx::Error) -> GamesError {
    if db::is_unique_violation(&error, &["players.user_id"]) {
        GamesError::Invalid(ValidationError::single("user_id", TAKEN))
    } else if db::is_unique_violation(&error, &["players.discord_id"]) {
        GamesError::Invalid(ValidationError::single("discord_id", TAKEN))
    } else if db::is_unique_violation(&error, &[]) {
        GamesError::Invalid(ValidationError::single("name", TAKEN))
    } else {
        GamesError::Database(error)
    }
}

/// `Games.create_player/2`: `name` and `archived_at` are cast; `discord_id` is taken as given
/// and `user_id` comes from trusted callers.
pub async fn create_player(
    conn: &mut SqliteConnection,
    attrs: &Value,
    user_id: Option<i64>,
) -> Result<Player, GamesError> {
    let mut cs = Changeset::new(attrs);
    let name = trim(cs.string("name").or(None));
    let archived_at = cs.datetime("archived_at").or(None);
    cs.required("name", name.as_ref());
    cs.length("name", name.as_deref(), Some(1), Some(100));
    let discord_id = attrs
        .get("discord_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(user_id) = user_id
        && !user_exists(conn, user_id).await?
    {
        cs.add_error("user_id", "does not exist");
    }
    cs.finish()?;
    let name = name.unwrap_or_default();
    let now = UtcDateTime::now();
    let id = sqlx::query_scalar!(
        r#"INSERT INTO players (name, user_id, discord_id, archived_at, inserted_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?) RETURNING id AS "id!: i64""#,
        name,
        user_id,
        discord_id,
        archived_at,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(unique_error)?;
    Ok(Player {
        id,
        name,
        user_id,
        discord_id,
        archived_at,
        avatar_url: None,
        inserted_at: now,
        updated_at: now,
    })
}

/// `Games.update_player/2`: renames or (un)archives.
pub async fn update_player(
    conn: &mut SqliteConnection,
    player: &Player,
    attrs: &Value,
) -> Result<Player, GamesError> {
    let mut cs = Changeset::new(attrs);
    let name = cs
        .string("name")
        .map(|name| name.trim().to_owned())
        .or(Some(player.name.clone()));
    let archived_at = cs.datetime("archived_at").or(player.archived_at);
    cs.required("name", name.as_ref());
    cs.length("name", name.as_deref(), Some(1), Some(100));
    cs.finish()?;
    let name = name.unwrap_or_default();
    if name == player.name && archived_at == player.archived_at {
        return Ok(player.clone());
    }
    let now = UtcDateTime::now();
    sqlx::query!(
        "UPDATE players SET name = ?, archived_at = ?, updated_at = ? WHERE id = ?",
        name,
        archived_at,
        now,
        player.id
    )
    .execute(&mut *conn)
    .await
    .map_err(unique_error)?;
    Ok(Player {
        name,
        archived_at,
        updated_at: now,
        ..player.clone()
    })
}

/// `Games.delete_player/1`.
///
/// Elixir's `Repo.delete/1` raised (a 500) when the player still had decks or seats, since
/// SQLite's foreign-key errors carry no constraint name; this returns Ecto's
/// `no_assoc_constraint` error instead.
pub async fn delete_player(conn: &mut SqliteConnection, player: &Player) -> Result<(), GamesError> {
    let refs = sqlx::query!(
        r#"SELECT EXISTS(SELECT 1 FROM decks WHERE player_id = ?1) AS "decks!: bool",
                  EXISTS(SELECT 1 FROM game_players WHERE player_id = ?1 OR eliminated_by_player_id = ?1) AS "seats!: bool""#,
        player.id
    )
    .fetch_one(&mut *conn)
    .await?;
    if refs.seats {
        return Err(ValidationError::single(
            "game_players",
            "are still associated with this entry",
        )
        .into());
    }
    if refs.decks {
        return Err(
            ValidationError::single("decks", "are still associated with this entry").into(),
        );
    }
    sqlx::query!("DELETE FROM players WHERE id = ?", player.id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The player whose name folds to `name`'s.
pub async fn find_player_by_name(
    conn: &mut SqliteConnection,
    name: &str,
) -> Result<Option<Player>, sqlx::Error> {
    let folded = fold_name(name);
    select_players!("WHERE lower(name) = ?", folded)
        .fetch_optional(&mut *conn)
        .await
}

/// `Games.find_or_create_player_by_name/2`: `attrs` may carry `discord_id` or
/// `archived_at`. A concurrent insert of the same name returns that player.
pub async fn find_or_create_player_by_name(
    conn: &mut SqliteConnection,
    name: &str,
    attrs: &Value,
) -> Result<Player, GamesError> {
    if let Some(player) = find_player_by_name(conn, name).await? {
        return Ok(player);
    }
    let mut attrs = attrs.as_object().cloned().unwrap_or_default();
    attrs.insert("name".into(), Value::String(name.to_owned()));
    match create_player(conn, &Value::Object(attrs), None).await {
        Err(GamesError::Invalid(errors)) => match find_player_by_name(conn, name).await? {
            Some(player) => Ok(player),
            None => Err(GamesError::Invalid(errors)),
        },
        other => other,
    }
}
