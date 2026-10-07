//! Filtered, paginated game history behind `GET /api/games`, newest first.
//!
//! Deck filters (`commander`, `colors`, `color`) match the `player_id` seat when a player is
//! chosen, so "Alice with Golgari" means Alice piloted Golgari; otherwise they match any
//! seat. Winner filters (`winner_id`, `winner_colors`, `winner_color`, `winner_seat`) all
//! describe one winning seat. Date, weekday, and hour filters read `played_at` in the `tz`
//! IANA zone, defaulting to UTC.

use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use crate::db::UtcDateTime;
use crate::local_time::{Zone, parse_date};

use super::Pagination;
use super::color_identity;
use super::model::{Game, load_games};

/// `GET /api/games` filters. Out-of-range numbers (such as `page=0` or `hour=25`) are
/// ignored.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GameFilters {
    /// Page number, from 1.
    pub page: Option<i64>,
    /// Games per page (default 20, at most 100).
    pub per_page: Option<i64>,
    /// IANA zone dates, weekdays, and hours are read in (UTC by default).
    pub tz: Option<String>,
    /// A player who sat in the game.
    pub player_id: Option<i64>,
    /// That player's result: `win`, `loss`, or `draw`.
    pub player_result: Option<String>,
    /// Commander name (of the player's seat when `player_id` is given).
    pub commander: Option<String>,
    /// Exact color identity (`WUBRG` letters or `C`).
    pub colors: Option<String>,
    /// A color the identity includes.
    pub color: Option<String>,
    /// The winner.
    pub winner_id: Option<i64>,
    /// The winner's exact color identity.
    pub winner_colors: Option<String>,
    /// A color the winner's identity includes.
    pub winner_color: Option<String>,
    /// The winner's seat number.
    pub winner_seat: Option<i64>,
    /// Another player at the table.
    pub opponent_id: Option<i64>,
    /// A deck played in the game.
    pub deck_id: Option<i64>,
    /// How the game was won.
    pub win_condition: Option<String>,
    /// Number of seats.
    pub player_count: Option<i64>,
    /// At least this many turns.
    pub min_turns: Option<i64>,
    /// At most this many turns.
    pub max_turns: Option<i64>,
    /// At least this many minutes.
    pub min_duration: Option<i64>,
    /// At most this many minutes.
    pub max_duration: Option<i64>,
    /// First day (`YYYY-MM-DD`, inclusive).
    pub date_from: Option<String>,
    /// Last day (`YYYY-MM-DD`, inclusive).
    pub date_to: Option<String>,
    /// Local weekday, 0 (Monday) to 6.
    pub weekday: Option<i64>,
    /// Local hour, 0 to 23.
    pub hour: Option<i64>,
}

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
            qb.push("(s.player_id = ")
                .push_bind(*id)
                .push(" AND s.result = ")
                .push_bind(*result)
                .push(")");
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
                qb.push(" AND instr(d.color_identity, ")
                    .push_bind(letter.to_string())
                    .push(") > 0");
            }
            qb.push(")");
        }
        Cond::IncludesColor(color) => {
            qb.push("instr(d.color_identity, ")
                .push_bind(color.clone())
                .push(") > 0");
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

fn positive(value: Option<i64>) -> Option<i64> {
    value.filter(|value| *value > 0)
}

fn in_range(value: Option<i64>, range: std::ops::RangeInclusive<i64>) -> Option<i64> {
    value.filter(|value| range.contains(value))
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
    ["W", "U", "B", "R", "G"]
        .contains(&color.as_str())
        .then_some(Cond::IncludesColor(color))
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
fn seat_conditions(opts: &GameFilters) -> Vec<Cond> {
    let deck_filters: Vec<Cond> = [
        commander(opts.commander.as_deref()),
        identity(opts.colors.as_deref()),
        includes_color(opts.color.as_deref()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let player_seat = match positive(opts.player_id) {
        Some(id) => {
            let mut conds = vec![Cond::Player(id, result(opts.player_result.as_deref()))];
            conds.extend(deck_filters);
            all(conds).into_iter().collect()
        }
        None => deck_filters,
    };
    let winner_seat: Vec<Cond> = [
        positive(opts.winner_id).map(|id| Cond::Player(id, None)),
        identity(opts.winner_colors.as_deref()),
        includes_color(opts.winner_color.as_deref()),
        positive(opts.winner_seat).map(Cond::SeatNumber),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut conds: Vec<Cond> = [
        all(winner_seat).map(|cond| Cond::Winning(Box::new(cond))),
        positive(opts.opponent_id).map(|id| Cond::Player(id, None)),
        positive(opts.deck_id).map(Cond::Deck),
    ]
    .into_iter()
    .flatten()
    .collect();
    conds.extend(player_seat);
    conds
}

/// Games matching `opts`, newest first, with pagination.
pub async fn list_games(
    conn: &mut SqliteConnection,
    opts: &GameFilters,
) -> Result<(Vec<Game>, Pagination), sqlx::Error> {
    let page = positive(opts.page).unwrap_or(1);
    let per_page = positive(opts.per_page).unwrap_or(20).min(100);
    let zone = Zone::parse(opts.tz.as_deref());

    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT g.id, g.played_at FROM games g WHERE 1 = 1");
    for cond in seat_conditions(opts) {
        qb.push(" AND g.id IN (SELECT s.game_id FROM game_players s LEFT JOIN decks d ON d.id = s.deck_id WHERE ");
        push(&mut qb, &cond);
        qb.push(")");
    }
    if let Some(condition) = opts
        .win_condition
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        qb.push(" AND g.win_condition = ")
            .push_bind(condition.to_owned());
    }
    if let Some(count) = positive(opts.player_count) {
        qb.push(
            " AND g.id IN (SELECT game_id FROM game_players GROUP BY game_id HAVING count(id) = ",
        )
        .push_bind(count)
        .push(")");
    }
    for (bound, column, op) in [
        (opts.min_turns, "g.turns", " >= "),
        (opts.max_turns, "g.turns", " <= "),
        (opts.min_duration, "g.duration_minutes", " >= "),
        (opts.max_duration, "g.duration_minutes", " <= "),
    ] {
        if let Some(bound) = positive(bound) {
            qb.push(" AND ").push(column).push(op).push_bind(bound);
        }
    }
    if let Some(start) = opts
        .date_from
        .as_deref()
        .and_then(parse_date)
        .and_then(|date| zone.start_of_day(date))
    {
        qb.push(" AND g.played_at >= ").push_bind(start);
    }
    if let Some(end) = opts
        .date_to
        .as_deref()
        .and_then(parse_date)
        .and_then(time::Date::next_day)
        .and_then(|date| zone.start_of_day(date))
    {
        qb.push(" AND g.played_at < ").push_bind(end);
    }
    qb.push(" ORDER BY g.played_at DESC, g.id DESC");
    let rows: Vec<(i64, UtcDateTime)> = qb.build_query_as().fetch_all(&mut *conn).await?;

    // SQLite has no time zone data, so weekday and hour are matched here.
    let weekday = in_range(opts.weekday, 0..=6);
    let hour = in_range(opts.hour, 0..=23);
    let ids: Vec<i64> = rows
        .into_iter()
        .filter(|(_, played_at)| {
            if weekday.is_none() && hour.is_none() {
                return true;
            }
            zone.weekday_and_hour(*played_at)
                .is_some_and(|(local_weekday, local_hour)| {
                    weekday.is_none_or(|wanted| wanted == local_weekday)
                        && hour.is_none_or(|wanted| wanted == local_hour)
                })
        })
        .map(|(id, _)| id)
        .collect();
    let total = i64::try_from(ids.len()).unwrap_or(i64::MAX);
    let skip =
        usize::try_from(page.saturating_sub(1).saturating_mul(per_page)).unwrap_or(usize::MAX);
    let take = usize::try_from(per_page).unwrap_or(usize::MAX);
    let page_ids: Vec<i64> = ids.into_iter().skip(skip).take(take).collect();
    let games = load_games(conn, &page_ids).await?;
    Ok((games, Pagination::new(page, per_page, total)))
}
