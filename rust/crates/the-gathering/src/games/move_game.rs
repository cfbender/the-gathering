//! Moving a game into another playgroup.
//!
//! Players and decks belong to one playgroup, so each seat is re-pointed to the
//! destination's matching player (by linked account, then Discord identity, then
//! case-insensitive name) and to that player's deck of the same name; players and decks
//! that do not exist there yet are created from the seat's.

use std::collections::HashMap;

use sqlx::SqliteConnection;

use crate::db::UtcDateTime;
use crate::validation::ValidationError;

use super::GamesError;
use super::merge_players::conflicting;
use super::model::{Deck, Game, Player, load_game, select_decks, select_players};

fn move_error(field: &str, message: impl Into<String>) -> GamesError {
    GamesError::Invalid(ValidationError::single(field, message))
}

/// Moves `game` into the playgroup `playgroup_id` and returns it as stored there.
pub async fn move_game(
    conn: &mut SqliteConnection,
    game: &Game,
    playgroup_id: i64,
) -> Result<Game, GamesError> {
    if game.playgroup_id == playgroup_id {
        return Err(move_error(
            "playgroup_id",
            "is already the game's playgroup",
        ));
    }
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM playgroups WHERE id = ?) AS "exists!: bool""#,
        playgroup_id
    )
    .fetch_one(&mut *conn)
    .await?;
    if !exists {
        return Err(move_error("playgroup_id", "does not exist"));
    }
    let now = UtcDateTime::now();
    let mut players: HashMap<i64, i64> = HashMap::new();
    let mut decks: HashMap<i64, i64> = HashMap::new();
    for seat in &game.seats {
        let target = match players.get(&seat.player_id) {
            Some(id) => *id,
            None => {
                let id = find_or_create_player(conn, &seat.player, playgroup_id, now).await?;
                players.insert(seat.player_id, id);
                id
            }
        };
        let deck = match &seat.deck {
            Some(deck) => Some(match decks.get(&deck.id) {
                Some(id) => *id,
                None => {
                    let id = find_or_copy_deck(conn, deck, target, now).await?;
                    decks.insert(deck.id, id);
                    id
                }
            }),
            None => None,
        };
        sqlx::query!(
            "UPDATE game_players SET player_id = ?, deck_id = ?, updated_at = ? WHERE id = ?",
            target,
            deck,
            now,
            seat.id
        )
        .execute(&mut *conn)
        .await?;
    }
    // Eliminations reference seated players, so every one already has a mapping.
    for seat in &game.seats {
        if let Some(killer) = seat.eliminated_by_player_id
            && let Some(target) = players.get(&killer)
        {
            sqlx::query!(
                "UPDATE game_players SET eliminated_by_player_id = ? WHERE id = ?",
                target,
                seat.id
            )
            .execute(&mut *conn)
            .await?;
        }
    }
    sqlx::query!(
        "UPDATE games SET playgroup_id = ?, updated_at = ? WHERE id = ?",
        playgroup_id,
        now,
        game.id
    )
    .execute(&mut *conn)
    .await
    .map_err(|error| {
        if crate::db::is_unique_violation(&error, &["games.external_id"]) {
            move_error("playgroup_id", "already has this game")
        } else {
            GamesError::from(error)
        }
    })?;
    load_game(conn, game.id).await?.ok_or(GamesError::NotFound)
}

/// The destination player for `player`: the same account, the same Discord identity, or
/// the same name (which then takes on the player's identity), created when none exists.
async fn find_or_create_player(
    conn: &mut SqliteConnection,
    player: &Player,
    playgroup_id: i64,
    now: UtcDateTime,
) -> Result<i64, GamesError> {
    if let Some(user_id) = player.user_id
        && let Some(found) = select_players!(
            "WHERE playgroup_id = ? AND user_id = ?",
            playgroup_id,
            user_id
        )
        .fetch_optional(&mut *conn)
        .await?
    {
        return Ok(found.id);
    }
    if let Some(discord_id) = &player.discord_id
        && let Some(found) = select_players!(
            "WHERE playgroup_id = ? AND discord_id = ?",
            playgroup_id,
            discord_id
        )
        .fetch_optional(&mut *conn)
        .await?
    {
        return Ok(found.id);
    }
    let name = player.name.trim();
    if let Some(found) = select_players!(
        "WHERE playgroup_id = ? AND name = ? COLLATE NOCASE",
        playgroup_id,
        name
    )
    .fetch_optional(&mut *conn)
    .await?
    {
        if conflicting(found.user_id.as_ref(), player.user_id.as_ref())
            || conflicting(found.discord_id.as_ref(), player.discord_id.as_ref())
        {
            return Err(move_error(
                "seats",
                format!("{} is a different player in that playgroup", found.name),
            ));
        }
        if (found.user_id.is_none() && player.user_id.is_some())
            || (found.discord_id.is_none() && player.discord_id.is_some())
        {
            let user_id = found.user_id.or(player.user_id);
            let discord_id = found.discord_id.or_else(|| player.discord_id.clone());
            sqlx::query!(
                "UPDATE players SET user_id = ?, discord_id = ?, updated_at = ? WHERE id = ?",
                user_id,
                discord_id,
                now,
                found.id
            )
            .execute(&mut *conn)
            .await?;
        }
        return Ok(found.id);
    }
    Ok(sqlx::query_scalar!(
        r#"INSERT INTO players (playgroup_id, name, user_id, discord_id, inserted_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?) RETURNING id AS "id!: i64""#,
        playgroup_id,
        name,
        player.user_id,
        player.discord_id,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// The destination player's deck named like `deck`, or a copy of `deck` made for them.
async fn find_or_copy_deck(
    conn: &mut SqliteConnection,
    deck: &Deck,
    player_id: i64,
    now: UtcDateTime,
) -> Result<i64, GamesError> {
    if let Some(found) = select_decks!(
        "WHERE player_id = ? AND name = ? COLLATE NOCASE",
        player_id,
        deck.name
    )
    .fetch_optional(&mut *conn)
    .await?
    {
        return Ok(found.id);
    }
    Ok(sqlx::query_scalar!(
        r#"INSERT INTO decks (player_id, name, commander_card_id, commander_name, commander_printing_id,
                              partner_card_id, partner_name, partner_printing_id, color_identity, decklist_url,
                              decklist_source, archived_at, skip_count, included_for_play, inserted_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?) RETURNING id AS "id!: i64""#,
        player_id,
        deck.name,
        deck.commander_card_id,
        deck.commander_name,
        deck.commander_printing_id,
        deck.partner_card_id,
        deck.partner_name,
        deck.partner_printing_id,
        deck.color_identity,
        deck.decklist_url,
        deck.decklist_source,
        deck.archived_at,
        deck.included_for_play,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await?)
}
