//! Folding one player into another and linking players to accounts
//! (`TheGathering.Games.MergePlayers`).

use std::collections::HashMap;

use sqlx::{Connection, SqliteConnection};

use crate::accounts::User;
use crate::db::UtcDateTime;
use crate::error::Errors;

use super::model::{Player, get_player, select_decks, select_players};
use super::{GamesError, fold_name};

const SEATED_MESSAGE: &str = "has a seat at an open webcam table; record that game and try again once the table closes (30 minutes after everyone leaves)";

fn merge_error(message: impl Into<String>) -> GamesError {
    GamesError::Invalid(Errors::single("merge", message))
}

fn conflicting<T: PartialEq>(a: Option<&T>, b: Option<&T>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a != b)
}

/// The message for merging a player seated at an open webcam table.
pub fn seated_error(source: &Player) -> GamesError {
    merge_error(format!("{} {SEATED_MESSAGE}", source.name))
}

/// `MergePlayers.run/2` once the webcam-table check passed: every seat and deck moves to
/// `target`, the account/Discord identity carries over, and `source` is deleted. Decks with
/// the same (case-folded) name collapse into `target`'s deck.
pub async fn merge_unseated(conn: &mut SqliteConnection, source: &Player, target: &Player) -> Result<Player, GamesError> {
    if source.id == target.id {
        return Err(GamesError::BadRequest);
    }
    let mut tx = conn.begin().await?;
    let shared = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM game_players a JOIN game_players b ON a.game_id = b.game_id
                         WHERE a.player_id = ? AND b.player_id = ?) AS "shared!: bool""#,
        source.id,
        target.id
    )
    .fetch_one(&mut *tx)
    .await?;
    if shared {
        return Err(merge_error("both players are seated in the same game"));
    }
    if conflicting(source.user_id.as_ref(), target.user_id.as_ref()) {
        return Err(merge_error("players belong to different accounts"));
    }
    if conflicting(source.discord_id.as_ref(), target.discord_id.as_ref()) {
        return Err(merge_error("players have different Discord identities"));
    }
    let now = UtcDateTime::now();
    // carry_identity/2
    sqlx::query!("UPDATE players SET user_id = NULL, discord_id = NULL, updated_at = ? WHERE id = ?", now, source.id)
        .execute(&mut *tx)
        .await?;
    let user_id = target.user_id.or(source.user_id);
    let discord_id = target.discord_id.clone().or_else(|| source.discord_id.clone());
    sqlx::query!(
        "UPDATE players SET user_id = ?, discord_id = ?, updated_at = ? WHERE id = ?",
        user_id,
        discord_id,
        now,
        target.id
    )
    .execute(&mut *tx)
    .await?;
    // move_decks/2
    let target_decks: HashMap<String, i64> = select_decks!("WHERE player_id = ?", target.id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|deck| (fold_name(&deck.name), deck.id))
        .collect();
    let source_decks = select_decks!("WHERE player_id = ?", source.id).fetch_all(&mut *tx).await?;
    for deck in source_decks {
        match target_decks.get(&fold_name(&deck.name)) {
            Some(existing) => {
                sqlx::query!("UPDATE game_players SET deck_id = ? WHERE deck_id = ?", existing, deck.id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query!("DELETE FROM decks WHERE id = ?", deck.id).execute(&mut *tx).await?;
            }
            None => {
                sqlx::query!("UPDATE decks SET player_id = ?, updated_at = ? WHERE id = ?", target.id, now, deck.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    // migrate_player_references/2
    sqlx::query!("UPDATE game_players SET player_id = ? WHERE player_id = ?", target.id, source.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query!(
        "UPDATE game_players SET eliminated_by_player_id = ? WHERE eliminated_by_player_id = ?",
        target.id,
        source.id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("DELETE FROM players WHERE id = ?", source.id).execute(&mut *tx).await?;
    let merged = get_player(&mut tx, target.id).await?.ok_or(GamesError::NotFound)?;
    tx.commit().await?;
    Ok(merged)
}

/// What linking a player to an account requires.
pub enum LinkPlan {
    /// Already linked, or linked now: the player.
    Done(Player),
    /// The account's current player must be merged into the player.
    Merge {
        /// The account's current player (the merge source).
        current: Player,
        /// The player being linked (the merge target).
        player: Player,
    },
}

/// `MergePlayers.link_to_user/2` up to the merge: links directly when the account has no
/// player, or says which merge links it.
pub async fn plan_link(conn: &mut SqliteConnection, player: &Player, user: &User) -> Result<LinkPlan, GamesError> {
    let player = get_player(conn, player.id).await?.ok_or(GamesError::NotFound)?;
    let current = select_players!("WHERE user_id = ?", user.id).fetch_optional(&mut *conn).await?;
    if current.as_ref().is_some_and(|current| current.id == player.id) {
        return Ok(LinkPlan::Done(player));
    }
    if conflicting(player.user_id.as_ref(), Some(&user.id)) {
        return Err(merge_error("players belong to different accounts"));
    }
    if conflicting(player.discord_id.as_ref(), user.discord_id.as_ref()) {
        return Err(merge_error("players have different Discord identities"));
    }
    if let Some(current) = current {
        return Ok(LinkPlan::Merge { current, player });
    }
    let discord_id = player.discord_id.clone().or_else(|| user.discord_id.clone());
    let now = UtcDateTime::now();
    sqlx::query!(
        "UPDATE players SET user_id = ?, discord_id = ?, updated_at = ? WHERE id = ?",
        user.id,
        discord_id,
        now,
        player.id
    )
    .execute(&mut *conn)
    .await
    .map_err(|error| {
        if crate::db::is_unique_violation(&error, &["players.discord_id"]) {
            GamesError::Invalid(Errors::single("discord_id", crate::changeset::TAKEN))
        } else {
            GamesError::Database(error)
        }
    })?;
    Ok(LinkPlan::Done(Player { user_id: Some(user.id), discord_id, updated_at: now, ..player }))
}
