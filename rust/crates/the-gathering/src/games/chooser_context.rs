//! The `deck` and `ctx` tables a deck chooser script receives, built from the member's
//! playable decks and every game they sat in. `docs/deck-chooser-scripts.md` documents each
//! field; keep the two in step.

use std::collections::HashSet;

use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;

use crate::db::UtcDateTime;
use crate::local_time::Zone;

use super::color_identity;
use super::deck_picker::Candidate;
use super::model::Player;

/// The newest games listed in `ctx.recent_games`.
const RECENT_GAMES: usize = 100;
/// The newest games listed in each `deck.games`.
const DECK_GAMES: usize = 25;
/// Games counted by `ctx.recent_colors`.
const RECENT_COLOR_GAMES: usize = 10;
/// The longest gap between games of one play session (and since the last one), in seconds.
const SESSION_GAP_SECONDS: i64 = 4 * 3600;
const COLORS: [&str; 5] = ["W", "U", "B", "R", "G"];

/// One seat of a game the member sat in.
#[derive(Clone, Debug)]
pub struct HistorySeat {
    game_id: i64,
    played_at: UtcDateTime,
    duration_minutes: Option<i64>,
    turns: Option<i64>,
    format: String,
    win_condition: Option<String>,
    player_id: i64,
    player_name: String,
    deck_id: Option<i64>,
    deck_name: Option<String>,
    commander_name: Option<String>,
    partner_name: Option<String>,
    color_identity: Option<String>,
    result: String,
    eliminated_turn: Option<i64>,
    kills: Option<i64>,
    mvp_card_name: Option<String>,
    eliminated_by: Option<String>,
}

/// A game's seats in seat order; games are newest first.
pub type HistoryGame = Vec<HistorySeat>;

/// Every game `player_id` sat in, newest first, with all of its seats.
pub async fn load_history(
    conn: &mut SqliteConnection,
    player_id: i64,
) -> Result<Vec<HistoryGame>, sqlx::Error> {
    let seats = sqlx::query_as!(
        HistorySeat,
        r#"SELECT g.id AS "game_id!", g.played_at AS "played_at!: UtcDateTime", g.duration_minutes, g.turns,
                  g.format AS "format!", g.win_condition, s.player_id AS "player_id!", p.name AS "player_name!",
                  s.deck_id, d.name AS "deck_name?", d.commander_name AS "commander_name?",
                  d.partner_name AS "partner_name?", d.color_identity AS "color_identity?",
                  s.result AS "result!", s.eliminated_turn, s.kills, s.mvp_card_name,
                  eb.name AS "eliminated_by?"
           FROM game_players s
           JOIN games g ON g.id = s.game_id
           JOIN players p ON p.id = s.player_id
           LEFT JOIN decks d ON d.id = s.deck_id
           LEFT JOIN players eb ON eb.id = s.eliminated_by_player_id
           WHERE s.game_id IN (SELECT game_id FROM game_players WHERE player_id = ?)
           ORDER BY g.played_at DESC, g.id DESC, s.seat, s.id"#,
        player_id
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut games: Vec<HistoryGame> = Vec::new();
    for seat in seats {
        match games.last_mut() {
            Some(game)
                if game
                    .first()
                    .is_some_and(|first| first.game_id == seat.game_id) =>
            {
                game.push(seat);
            }
            _ => games.push(vec![seat]),
        }
    }
    Ok(games)
}

/// Where and when the script runs.
struct View<'a> {
    zone: &'a Zone,
    now: UtcDateTime,
    today: String,
    session: HashSet<i64>,
    player_id: i64,
}

fn float(value: i64) -> f64 {
    i32::try_from(value).map_or(f64::from(i32::MAX), f64::from)
}

/// Rounded to two decimals so previews stay readable.
fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn hours_between(later: UtcDateTime, earlier: UtcDateTime) -> f64 {
    round(float(later.unix().saturating_sub(earlier.unix())) / 3600.0)
}

fn ratio(part: usize, whole: usize) -> Value {
    match (u32::try_from(part), u32::try_from(whole)) {
        (Ok(part), Ok(whole)) if whole > 0 => json!(round(f64::from(part) / f64::from(whole))),
        _ => Value::Null,
    }
}

fn average(values: impl Iterator<Item = i64>) -> Value {
    let (sum, count) = values.fold((0_i64, 0_i64), |(sum, count), value| {
        (sum.saturating_add(value), count.saturating_add(1))
    });
    if count == 0 {
        Value::Null
    } else {
        json!(round(float(sum) / float(count)))
    }
}

const WEEKDAYS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];

/// A `time` table: the instant in the browser's zone.
fn local_time(zone: &Zone, at: UtcDateTime) -> Value {
    let Some(local) = zone.zoned(at) else {
        return json!({ "unix": at.unix() });
    };
    let weekday = local.weekday().to_sunday_one_offset();
    let name = usize::try_from(local.weekday().to_sunday_zero_offset())
        .ok()
        .and_then(|index| WEEKDAYS.get(index))
        .copied()
        .unwrap_or_default();
    json!({
        "unix": at.unix(),
        "date": local.date().to_string(),
        "year": local.year(),
        "month": local.month(),
        "day": local.day(),
        "hour": local.hour(),
        "minute": local.minute(),
        "weekday": weekday,
        "weekday_name": name,
        "weekend": weekday == 1 || weekday == 7,
    })
}

fn local_date(zone: &Zone, at: UtcDateTime) -> Option<String> {
    zone.zoned(at).map(|local| local.date().to_string())
}

/// `color_identity` (WUBRG order), `colors` (list), `color_set` (`{W = true}`), `color_name`,
/// and `color_count` for a deck identity.
fn color_fields(identity: &str, into: &mut Map<String, Value>) {
    let canonical = color_identity::canonical(identity);
    let colors: Vec<String> = canonical.chars().map(String::from).collect();
    let set: Map<String, Value> = colors
        .iter()
        .map(|color| (color.clone(), Value::Bool(true)))
        .collect();
    into.insert("color_name".into(), json!(color_identity::name(&canonical)));
    into.insert("color_count".into(), json!(colors.len()));
    into.insert("colors".into(), json!(colors));
    into.insert("color_set".into(), Value::Object(set));
    into.insert("color_identity".into(), json!(canonical));
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn my_seat(game: &[HistorySeat], player_id: i64) -> Option<&HistorySeat> {
    game.iter().find(|seat| seat.player_id == player_id)
}

fn opponent_json(seat: &HistorySeat) -> Value {
    let mut opponent = object(json!({
        "player_id": seat.player_id,
        "name": seat.player_name,
        "deck_id": seat.deck_id,
        "deck_name": seat.deck_name,
        "commander": seat.commander_name,
        "partner": seat.partner_name,
        "result": seat.result,
        "won": seat.result == "win",
        "kills": seat.kills,
        "eliminated_turn": seat.eliminated_turn,
    }));
    color_fields(
        seat.color_identity.as_deref().unwrap_or_default(),
        &mut opponent,
    );
    Value::Object(opponent)
}

/// A `game` table from the member's seat.
fn game_json(game: &[HistorySeat], me: &HistorySeat, view: &View) -> Value {
    let hours_ago = hours_between(view.now, me.played_at);
    let mut record = object(json!({
        "id": me.game_id,
        "played_at": me.played_at.unix(),
        "local": local_time(view.zone, me.played_at),
        "hours_ago": hours_ago,
        "days_ago": round(hours_ago / 24.0),
        "today": local_date(view.zone, me.played_at).as_deref() == Some(view.today.as_str()),
        "in_session": view.session.contains(&me.game_id),
        "format": me.format,
        "win_condition": me.win_condition,
        "duration_minutes": me.duration_minutes,
        "turns": me.turns,
        "players": game.len(),
        "result": me.result,
        "won": me.result == "win",
        "lost": me.result == "loss",
        "drew": me.result == "draw",
        "deck_id": me.deck_id,
        "deck_name": me.deck_name,
        "commander": me.commander_name,
        "partner": me.partner_name,
        "eliminated_turn": me.eliminated_turn,
        "eliminated_by": me.eliminated_by,
        "kills": me.kills,
        "mvp_card": me.mvp_card_name,
        "winners": game.iter().filter(|seat| seat.result == "win").map(|seat| seat.player_name.as_str()).collect::<Vec<_>>(),
        "opponents": game.iter().filter(|seat| seat.player_id != view.player_id).map(opponent_json).collect::<Vec<_>>(),
    }));
    color_fields(
        me.color_identity.as_deref().unwrap_or_default(),
        &mut record,
    );
    Value::Object(record)
}

/// The current result streak of games ordered newest first.
fn streak(seats: &[&HistorySeat]) -> Value {
    let Some(latest) = seats.first() else {
        return Value::Null;
    };
    let length = seats
        .iter()
        .take_while(|seat| seat.result == latest.result)
        .count();
    json!({ "result": latest.result, "length": length })
}

/// A `deck` table.
fn deck_json(candidate: &Candidate, history: &[HistoryGame], view: &View) -> Value {
    let deck = &candidate.deck;
    let games: Vec<(&HistoryGame, &HistorySeat)> = history
        .iter()
        .filter_map(|game| Some((game, my_seat(game, view.player_id)?)))
        .filter(|(_, me)| me.deck_id == Some(deck.id))
        .collect();
    let seats: Vec<&HistorySeat> = games.iter().map(|(_, me)| *me).collect();
    let count = |result: &str| seats.iter().filter(|seat| seat.result == result).count();
    let within = |seconds: i64| {
        seats
            .iter()
            .filter(|seat| view.now.unix().saturating_sub(seat.played_at.unix()) <= seconds)
            .count()
    };
    let hours_since_played = candidate
        .last_played_at
        .map(|played_at| hours_between(view.now, played_at));
    let records: Vec<Value> = games
        .iter()
        .take(DECK_GAMES)
        .map(|(game, me)| game_json(game, me, view))
        .collect();
    let added_hours = hours_between(view.now, deck.inserted_at);
    let mut table = object(json!({
        "id": deck.id,
        "name": deck.name,
        "commander": deck.commander_name,
        "partner": deck.partner_name,
        "colorless": color_identity::canonical(&deck.color_identity).is_empty(),
        "decklist_source": deck.decklist_source,
        "added_at": deck.inserted_at.unix(),
        "days_since_added": round(added_hours / 24.0),
        "plays": candidate.play_count,
        "skips": deck.skip_count,
        "wins": count("win"),
        "losses": count("loss"),
        "draws": count("draw"),
        "win_rate": ratio(count("win"), seats.len()),
        "base_weight": candidate.weight,
        "never_played": candidate.last_played_at.is_none(),
        "last_played_at": candidate.last_played_at.map(UtcDateTime::unix),
        "hours_since_played": hours_since_played,
        "days_since_played": hours_since_played.map(|hours| round(hours / 24.0)),
        "plays_today": seats.iter().filter(|seat| local_date(view.zone, seat.played_at).as_deref() == Some(view.today.as_str())).count(),
        "plays_this_session": seats.iter().filter(|seat| view.session.contains(&seat.game_id)).count(),
        "plays_last_7_days": within(7 * 24 * 3600),
        "plays_last_30_days": within(30 * 24 * 3600),
        "avg_duration_minutes": average(seats.iter().filter_map(|seat| seat.duration_minutes)),
        "avg_turns": average(seats.iter().filter_map(|seat| seat.turns)),
        "streak": streak(&seats),
        "last_game": records.first(),
        "games": records,
    }));
    color_fields(&deck.color_identity, &mut table);
    Value::Object(table)
}

/// Games of the current play session: the newest game within four hours of now, and each
/// earlier game within four hours of the one after it.
fn session(history: &[HistoryGame], now: UtcDateTime) -> HashSet<i64> {
    let mut ids = HashSet::new();
    let mut cursor = now;
    for game in history {
        let Some(first) = game.first() else { continue };
        if cursor.unix().saturating_sub(first.played_at.unix()) > SESSION_GAP_SECONDS {
            break;
        }
        ids.insert(first.game_id);
        cursor = cursor.min(first.played_at);
    }
    ids
}

/// The `ctx` table, with `ctx.decks` holding a `deck` table per candidate in order.
pub fn build(
    player: &Player,
    candidates: &[Candidate],
    history: &[HistoryGame],
    now: UtcDateTime,
    zone: &Zone,
    excluded_deck_id: Option<i64>,
) -> Value {
    let view = View {
        zone,
        now,
        today: local_date(zone, now).unwrap_or_default(),
        session: session(history, now),
        player_id: player.id,
    };
    let mine: Vec<(&HistoryGame, &HistorySeat)> = history
        .iter()
        .filter_map(|game| Some((game, my_seat(game, player.id)?)))
        .collect();
    let records: Vec<Value> = mine
        .iter()
        .take(RECENT_GAMES)
        .map(|(game, me)| game_json(game, me, &view))
        .collect();
    let today: Vec<Value> = records
        .iter()
        .filter(|game| game["today"] == true)
        .cloned()
        .collect();
    let session_games: Vec<Value> = records
        .iter()
        .filter(|game| game["in_session"] == true)
        .cloned()
        .collect();
    let mut recent_colors: Map<String, Value> = COLORS
        .iter()
        .map(|color| ((*color).into(), json!(0)))
        .collect();
    recent_colors.insert("C".into(), json!(0));
    for (_, me) in mine.iter().take(RECENT_COLOR_GAMES) {
        let canonical = color_identity::canonical(me.color_identity.as_deref().unwrap_or_default());
        let keys: Vec<String> = if canonical.is_empty() {
            vec!["C".into()]
        } else {
            canonical.chars().map(String::from).collect()
        };
        for key in keys {
            let count = recent_colors.get(&key).and_then(Value::as_i64).unwrap_or(0);
            recent_colors.insert(key, json!(count.saturating_add(1)));
        }
    }
    let results = |result: &str| mine.iter().filter(|(_, me)| me.result == result).count();
    json!({
        "now": local_time(zone, now),
        "timezone": zone.name(),
        "player": { "id": player.id, "name": player.name },
        "excluded_deck_id": excluded_deck_id,
        "games_today": today.len(),
        "session_games": session_games.len(),
        "minutes_since_last_game": mine.first().map(|(_, me)| round(float(now.unix().saturating_sub(me.played_at.unix())) / 60.0)),
        "last_game": records.first(),
        "today": today,
        "session": session_games,
        "recent_games": records,
        "recent_colors": recent_colors,
        "stats": {
            "games": mine.len(),
            "wins": results("win"),
            "losses": results("loss"),
            "draws": results("draw"),
            "win_rate": ratio(results("win"), mine.len()),
            "decks": candidates.len(),
        },
        "decks": candidates.iter().map(|candidate| deck_json(candidate, history, &view)).collect::<Vec<_>>(),
    })
}
