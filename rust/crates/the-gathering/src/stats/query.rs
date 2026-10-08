//! Scoped and date-filtered queries shared by statistics views.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;
use time::Date;

use crate::db::UtcDateTime;
use crate::games::{Game, load_games};
use crate::local_time::{Zone, parse_date};

/// The days a statistics view covers: inclusive `date_from`/`date_to` (`YYYY-MM-DD`) read
/// in the `tz` zone (UTC by default). Unparseable dates are ignored.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct DateRange {
    /// First day.
    pub date_from: Option<String>,
    /// Last day.
    pub date_to: Option<String>,
    /// IANA zone.
    pub tz: Option<String>,
}

/// The range's bounds as `[from, to)` instants.
pub fn date_bounds(params: &DateRange) -> (Option<UtcDateTime>, Option<UtcDateTime>) {
    let zone = Zone::parse(params.tz.as_deref());
    let from = params
        .date_from
        .as_deref()
        .and_then(parse_date)
        .and_then(|date| zone.start_of_day(date));
    let to = params
        .date_to
        .as_deref()
        .and_then(parse_date)
        .and_then(Date::next_day)
        .and_then(|date| zone.start_of_day(date));
    (from, to)
}

/// The range without its start, for views (Elo) that replay every earlier game.
pub fn without_date_from(params: &DateRange) -> DateRange {
    DateRange {
        date_from: None,
        ..params.clone()
    }
}

/// The first local day and the instant it begins, when `date_from` is valid.
pub fn window_start(params: &DateRange) -> Option<(Date, UtcDateTime)> {
    let date = params.date_from.as_deref().and_then(parse_date)?;
    let starts_at = Zone::parse(params.tz.as_deref()).start_of_day(date)?;
    Some((date, starts_at))
}

/// Games in the date range (optionally those a player or deck sat in), newest
/// first, with seats, players, and decks.
pub async fn games(
    conn: &mut SqliteConnection,
    params: &DateRange,
    player_id: Option<i64>,
    deck_id: Option<i64>,
) -> Result<Vec<Game>, sqlx::Error> {
    let (from, to) = date_bounds(params);
    let ids = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM games
           WHERE (?1 IS NULL OR played_at >= ?1) AND (?2 IS NULL OR played_at < ?2)
             AND (?3 IS NULL OR id IN (SELECT game_id FROM game_players WHERE player_id = ?3))
             AND (?4 IS NULL OR id IN (SELECT game_id FROM game_players WHERE deck_id = ?4))
           ORDER BY played_at DESC, id DESC"#,
        from,
        to,
        player_id,
        deck_id
    )
    .fetch_all(&mut *conn)
    .await?;
    load_games(conn, &ids).await
}

/// The newest `limit` of `game_ids`.
pub async fn recent_games(
    conn: &mut SqliteConnection,
    game_ids: &[i64],
    limit: i64,
) -> Result<Vec<Game>, sqlx::Error> {
    let ids_json = serde_json::to_string(game_ids).unwrap_or_else(|_| "[]".into());
    let ids = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM games WHERE id IN (SELECT value FROM json_each(?))
           ORDER BY played_at DESC, id DESC LIMIT ?"#,
        ids_json,
        limit
    )
    .fetch_all(&mut *conn)
    .await?;
    load_games(conn, &ids).await
}

/// A stored commander slot: `(card id, name)`.
pub type Reference = (Option<String>, Option<String>);

async fn all_rows(conn: &mut SqliteConnection) -> Result<Vec<[Reference; 2]>, sqlx::Error> {
    Ok(sqlx::query!(
        "SELECT commander_card_id, commander_name, partner_card_id, partner_name FROM decks"
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|row| {
        [
            (row.commander_card_id, Some(row.commander_name)),
            (row.partner_card_id, row.partner_name),
        ]
    })
    .collect())
}

fn unique(references: impl IntoIterator<Item = Reference>) -> Vec<Reference> {
    let mut seen = HashSet::new();
    references
        .into_iter()
        .filter(|reference| seen.insert(reference.clone()))
        .collect()
}

/// Every stored commander reference except empty slots.
async fn all_commander_references(
    conn: &mut SqliteConnection,
) -> Result<Vec<Reference>, sqlx::Error> {
    Ok(unique(all_rows(conn).await?.into_iter().flatten().filter(
        |(id, name)| id.is_some() || name.as_deref().is_some_and(|name| !name.is_empty()),
    )))
}

fn reference_matches(reference: &Reference, id: &str, normalized: &str) -> bool {
    reference.0.as_deref() == Some(id)
        || reference
            .1
            .as_deref()
            .is_some_and(|name| lotus::normalize_name(name) == normalized)
}

/// Stored references that match `id` as an id or normalized name.
pub async fn commander_references(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<Vec<Reference>, sqlx::Error> {
    let normalized = lotus::normalize_name(id);
    let downcased = id.to_lowercase();
    let rows = sqlx::query!(
        "SELECT commander_card_id, commander_name, partner_card_id, partner_name FROM decks
         WHERE commander_card_id = ?1 OR partner_card_id = ?1 OR lower(commander_name) = ?2 OR lower(partner_name) = ?2",
        id,
        downcased
    )
    .fetch_all(&mut *conn)
    .await?;
    let direct = unique(
        rows.into_iter()
            .flat_map(|row| {
                [
                    (row.commander_card_id, Some(row.commander_name)),
                    (row.partner_card_id, row.partner_name),
                ]
            })
            .filter(|reference| reference_matches(reference, id, &normalized)),
    );
    if !direct.is_empty() {
        return Ok(direct);
    }
    Ok(all_commander_references(conn)
        .await?
        .into_iter()
        .filter(|reference| reference_matches(reference, id, &normalized))
        .collect())
}

/// Every stored id and exact name spelling that canonicalizes to the
/// given ids or names.
pub async fn commander_aliases(
    conn: &mut SqliteConnection,
    ids: &[String],
    names: &[String],
) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
    let normalized: HashSet<String> = names
        .iter()
        .map(|name| lotus::normalize_name(name))
        .collect();
    let references: Vec<Reference> = all_commander_references(conn)
        .await?
        .into_iter()
        .filter(|(id, name)| {
            id.as_ref().is_some_and(|id| ids.contains(id))
                || name
                    .as_deref()
                    .is_some_and(|name| normalized.contains(&lotus::normalize_name(name)))
        })
        .collect();
    let mut stored_ids: Vec<String> = Vec::new();
    let mut stored_names: Vec<String> = Vec::new();
    for (id, name) in references {
        if let Some(id) = id
            && !stored_ids.contains(&id)
        {
            stored_ids.push(id);
        }
        if let Some(name) = name
            && !stored_names.contains(&name)
        {
            stored_names.push(name);
        }
    }
    Ok((stored_ids, stored_names))
}

/// Per opponent across `game_ids`, their record in those games and
/// `beaten`, how many of those games a tracked seat won against them.
pub async fn opponent_counts(
    conn: &mut SqliteConnection,
    game_ids: &[i64],
    tracked_seat_ids: &[i64],
) -> Result<Vec<Map<String, Value>>, sqlx::Error> {
    let games_json = serde_json::to_string(game_ids).unwrap_or_else(|_| "[]".into());
    let seats_json = serde_json::to_string(tracked_seat_ids).unwrap_or_else(|_| "[]".into());
    let rows = sqlx::query!(
        r#"SELECT p.id AS "id!: i64", p.name, u.avatar_url AS "avatar_url?: String",
                  count(s.id) AS "games!: i64",
                  SUM(CASE WHEN s.result = 'win' THEN 1 ELSE 0 END) AS "wins!: i64",
                  SUM(CASE WHEN s.result = 'loss' THEN 1 ELSE 0 END) AS "losses!: i64",
                  SUM(CASE WHEN s.result = 'draw' THEN 1 ELSE 0 END) AS "draws!: i64",
                  count(w.id) AS "beaten!: i64"
           FROM game_players s
           JOIN players p ON p.id = s.player_id
           LEFT JOIN users u ON u.id = p.user_id
           LEFT JOIN game_players w ON w.game_id = s.game_id AND w.result = 'win'
                AND w.id IN (SELECT value FROM json_each(?2))
           WHERE s.game_id IN (SELECT value FROM json_each(?1)) AND s.id NOT IN (SELECT value FROM json_each(?2))
           GROUP BY p.id, p.name, u.avatar_url
           ORDER BY p.id"#,
        games_json,
        seats_json
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let mut object = Map::new();
            object.insert("id".into(), json!(row.id));
            object.insert("name".into(), json!(row.name));
            object.insert("avatar_url".into(), json!(row.avatar_url));
            object.insert("games".into(), json!(row.games));
            object.insert("wins".into(), json!(row.wins));
            object.insert("losses".into(), json!(row.losses));
            object.insert("draws".into(), json!(row.draws));
            object.insert("beaten".into(), json!(row.beaten));
            object
        })
        .collect())
}

/// The administrator's `detailed_stats_from` cutoff.
pub async fn detailed_stats_from(
    conn: &mut SqliteConnection,
) -> Result<Option<crate::db::IsoDate>, sqlx::Error> {
    Ok(sqlx::query_scalar!(
        r#"SELECT detailed_stats_from AS "detailed_stats_from: crate::db::IsoDate" FROM server_settings WHERE id = 1"#
    )
    .fetch_optional(&mut *conn)
    .await?
    .flatten())
}

/// Games on or after the cutoff's UTC day (all games without a cutoff).
pub fn detailed(games: &[Game], cutoff: Option<crate::db::IsoDate>) -> Vec<&Game> {
    games
        .iter()
        .filter(|game| cutoff.is_none_or(|cutoff| game.played_at.date() >= cutoff.0))
        .collect()
}
