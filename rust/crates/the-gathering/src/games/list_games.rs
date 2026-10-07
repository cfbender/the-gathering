//! Filtered, paginated game history behind `GET /api/games`, newest first
//! (`TheGathering.Games.ListGames`).
//!
//! Deck filters (`commander`, `colors`, `color`) match the `player_id` seat when a player is
//! chosen, so "Alice with Golgari" means Alice piloted Golgari; otherwise they match any
//! seat. Winner filters (`winner_id`, `winner_colors`, `winner_color`, `winner_seat`) all
//! describe one winning seat. Date, weekday, and hour filters read `played_at` in the `tz`
//! IANA zone, defaulting to UTC.

use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use crate::db::UtcDateTime;
use crate::local_time::{Zone, parse_date};

use super::Pagination;
use super::color_identity;
use super::model::{Game, load_games};

/// A condition one seat must meet.
#[derive(Clone, Debug)]
enum Cond {
    Player(i64, Option<&'static str>),
    Winning(Box<Cond>),
    Deck(i64),
    SeatNumber(i64),
    Commander(String),
    Colorless,
    Identity(String),
    IncludesColor(String),
    All(Vec<Cond>),
}

fn push(qb: &mut QueryBuilder<Sqlite>, cond: &Cond) {
    match cond {
        Cond::Player(id, None) => {
            qb.push("s.player_id = ").push_bind(*id);
        }
        Cond::Player(id, Some(result)) => {
            qb.push("(s.player_id = ").push_bind(*id).push(" AND s.result = ").push_bind(*result).push(")");
        }
        Cond::Winning(inner) => {
            qb.push("(s.result = 'win' AND ");
            push(qb, inner);
            qb.push(")");
        }
        Cond::Deck(id) => {
            qb.push("s.deck_id = ").push_bind(*id);
        }
        Cond::SeatNumber(number) => {
            qb.push("s.seat = ").push_bind(*number);
        }
        Cond::Commander(needle) => {
            qb.push("(instr(lower(d.commander_name), ")
                .push_bind(needle.clone())
                .push(") > 0 OR instr(lower(coalesce(d.partner_name, '')), ")
                .push_bind(needle.clone())
                .push(") > 0)");
        }
        Cond::Colorless => {
            qb.push("(d.id IS NOT NULL AND coalesce(d.color_identity, '') = '')");
        }
        Cond::Identity(canonical) => {
            let length = i64::try_from(canonical.chars().count()).unwrap_or_default();
            qb.push("(length(d.color_identity) = ").push_bind(length);
            for letter in canonical.chars() {
                qb.push(" AND instr(d.color_identity, ").push_bind(letter.to_string()).push(") > 0");
            }
            qb.push(")");
        }
        Cond::IncludesColor(color) => {
            qb.push("instr(d.color_identity, ").push_bind(color.clone()).push(") > 0");
        }
        Cond::All(conds) => {
            qb.push("(");
            for (index, cond) in conds.iter().enumerate() {
                if index > 0 {
                    qb.push(" AND ");
                }
                push(qb, cond);
            }
            qb.push(")");
        }
    }
}

fn all(mut conds: Vec<Cond>) -> Option<Cond> {
    match conds.len() {
        0 => None,
        1 => conds.pop(),
        _ => Some(Cond::All(conds)),
    }
}

fn integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn positive(value: Option<&Value>) -> Option<i64> {
    integer(value).filter(|value| *value > 0)
}

fn in_range(value: Option<&Value>, range: std::ops::RangeInclusive<i64>) -> Option<i64> {
    integer(value).filter(|value| range.contains(value))
}

fn text<'a>(opts: &'a Value, key: &str) -> Option<&'a str> {
    opts.get(key).and_then(Value::as_str)
}

fn commander(name: Option<&str>) -> Option<Cond> {
    let trimmed = name?.trim();
    (!trimmed.is_empty()).then(|| Cond::Commander(trimmed.to_lowercase()))
}

/// Exact color identity: canonical WUBRG letters, or `C` for colorless. Stored identities
/// are unique WUBRG letters but not ordered, so compare length and membership.
fn identity(value: Option<&str>) -> Option<Cond> {
    let letters = value?.trim().to_uppercase();
    if letters == "C" {
        return Some(Cond::Colorless);
    }
    let valid = !letters.is_empty() && letters.chars().all(|letter| "WUBRG".contains(letter));
    valid.then(|| Cond::Identity(color_identity::canonical(&letters)))
}

fn includes_color(value: Option<&str>) -> Option<Cond> {
    let color = value?.trim().to_uppercase();
    ["W", "U", "B", "R", "G"].contains(&color.as_str()).then_some(Cond::IncludesColor(color))
}

fn result(value: Option<&str>) -> Option<&'static str> {
    match value? {
        "win" => Some("win"),
        "loss" => Some("loss"),
        "draw" => Some("draw"),
        _ => None,
    }
}

/// Each condition must hold for one seat of the game.
fn seat_conditions(opts: &Value) -> Vec<Cond> {
    let deck_filters: Vec<Cond> = [
        commander(text(opts, "commander")),
        identity(text(opts, "colors")),
        includes_color(text(opts, "color")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let player_seat = match positive(opts.get("player_id")) {
        Some(id) => {
            let mut conds = vec![Cond::Player(id, result(text(opts, "player_result")))];
            conds.extend(deck_filters);
            all(conds).into_iter().collect()
        }
        None => deck_filters,
    };
    let winner_seat: Vec<Cond> = [
        positive(opts.get("winner_id")).map(|id| Cond::Player(id, None)),
        identity(text(opts, "winner_colors")),
        includes_color(text(opts, "winner_color")),
        positive(opts.get("winner_seat")).map(Cond::SeatNumber),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut conds: Vec<Cond> = [
        all(winner_seat).map(|cond| Cond::Winning(Box::new(cond))),
        positive(opts.get("opponent_id")).map(|id| Cond::Player(id, None)),
        positive(opts.get("deck_id")).map(Cond::Deck),
    ]
    .into_iter()
    .flatten()
    .collect();
    conds.extend(player_seat);
    conds
}

/// `ListGames.call/1`: games matching `opts` (string params as the controller passes them,
/// or JSON numbers), newest first, with pagination.
pub async fn list_games(conn: &mut SqliteConnection, opts: &Value) -> Result<(Vec<Game>, Pagination), sqlx::Error> {
    let page = positive(opts.get("page")).unwrap_or(1);
    let per_page = positive(opts.get("per_page")).unwrap_or(20).min(100);
    let zone = Zone::parse(text(opts, "tz"));

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT g.id, g.played_at FROM games g WHERE 1 = 1");
    for cond in seat_conditions(opts) {
        qb.push(" AND g.id IN (SELECT s.game_id FROM game_players s LEFT JOIN decks d ON d.id = s.deck_id WHERE ");
        push(&mut qb, &cond);
        qb.push(")");
    }
    if let Some(condition) = text(opts, "win_condition").filter(|value| !value.is_empty()) {
        qb.push(" AND g.win_condition = ").push_bind(condition.to_owned());
    }
    if let Some(count) = positive(opts.get("player_count")) {
        qb.push(" AND g.id IN (SELECT game_id FROM game_players GROUP BY game_id HAVING count(id) = ")
            .push_bind(count)
            .push(")");
    }
    for (key, column, op) in [
        ("min_turns", "g.turns", " >= "),
        ("max_turns", "g.turns", " <= "),
        ("min_duration", "g.duration_minutes", " >= "),
        ("max_duration", "g.duration_minutes", " <= "),
    ] {
        if let Some(bound) = positive(opts.get(key)) {
            qb.push(" AND ").push(column).push(op).push_bind(bound);
        }
    }
    if let Some(start) = text(opts, "date_from").and_then(parse_date).and_then(|date| zone.start_of_day(date)) {
        qb.push(" AND g.played_at >= ").push_bind(start);
    }
    if let Some(end) = text(opts, "date_to")
        .and_then(parse_date)
        .and_then(time::Date::next_day)
        .and_then(|date| zone.start_of_day(date))
    {
        qb.push(" AND g.played_at < ").push_bind(end);
    }
    qb.push(" ORDER BY g.played_at DESC, g.id DESC");
    let rows: Vec<(i64, UtcDateTime)> = qb.build_query_as().fetch_all(&mut *conn).await?;

    // SQLite has no time zone data, so weekday and hour are matched here.
    let weekday = in_range(opts.get("weekday"), 0..=6);
    let hour = in_range(opts.get("hour"), 0..=23);
    let ids: Vec<i64> = rows
        .into_iter()
        .filter(|(_, played_at)| {
            if weekday.is_none() && hour.is_none() {
                return true;
            }
            zone.weekday_and_hour(*played_at).is_some_and(|(local_weekday, local_hour)| {
                weekday.is_none_or(|wanted| wanted == local_weekday) && hour.is_none_or(|wanted| wanted == local_hour)
            })
        })
        .map(|(id, _)| id)
        .collect();
    let total = i64::try_from(ids.len()).unwrap_or(i64::MAX);
    let skip = usize::try_from(page.saturating_sub(1).saturating_mul(per_page)).unwrap_or(usize::MAX);
    let take = usize::try_from(per_page).unwrap_or(usize::MAX);
    let page_ids: Vec<i64> = ids.into_iter().skip(skip).take(take).collect();
    let games = load_games(conn, &page_ids).await?;
    Ok((games, Pagination::new(page, per_page, total)))
}
