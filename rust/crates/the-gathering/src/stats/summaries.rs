//! Response shaping shared by statistics views.

use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::catalog::{self, CardRef, CardSummaries};
use crate::games::{Deck, Game, GameResult, Player, Seat};

use super::records::{Object, histogram, sort_records, tracked_result};

/// `entity/1` of a player: `{id, name}`.
pub fn player(player: &Player) -> Object {
    let mut object = Object::new();
    object.insert("id".into(), json!(player.id));
    object.insert("name".into(), json!(player.name));
    object
}

/// `entity/1` of a deck: `{id, name, commander_name, color_identity}`, plus `retired: true`
/// when archived.
pub fn deck_entity(deck: &Deck) -> Object {
    let mut object = Object::new();
    object.insert("id".into(), json!(deck.id));
    object.insert("name".into(), json!(deck.name));
    object.insert("commander_name".into(), json!(deck.commander_name));
    object.insert("color_identity".into(), json!(deck.color_identity));
    if deck.archived_at.is_some() {
        object.insert("retired".into(), json!(true));
    }
    object
}

/// `recent_game/2`: `tracked` are the tracked seats' results (empty for none).
pub fn recent_game(game: &Game, tracked: &[GameResult]) -> Object {
    let mut object = Object::new();
    object.insert("id".into(), json!(game.id));
    object.insert("played_at".into(), json!(game.played_at));
    object.insert("duration_minutes".into(), json!(game.duration_minutes));
    object.insert("turns".into(), json!(game.turns));
    object.insert("result".into(), json!(tracked_result(tracked)));
    object.insert(
        "winner".into(),
        game.winner()
            .map_or(Value::Null, |winner| Value::Object(player(&winner.player))),
    );
    object.insert("players".into(), json!(game.seats.len()));
    object
}

type Slot = (Option<String>, Option<String>, Option<String>);

fn commander_refs(deck: Option<&Deck>) -> Vec<Slot> {
    match deck {
        None => vec![(None, None, None)],
        Some(deck) => {
            let commander = (
                deck.commander_card_id.clone(),
                Some(deck.commander_name.clone()),
                deck.commander_printing_id.clone(),
            );
            match &deck.partner_name {
                Some(partner) => vec![
                    commander,
                    (
                        deck.partner_card_id.clone(),
                        Some(partner.clone()),
                        deck.partner_printing_id.clone(),
                    ),
                ],
                None => vec![commander],
            }
        }
    }
}

/// `recent_games/1`: recent games with one winner-first portrait per seat; partner pairings
/// carry both crops.
pub async fn recent_games(
    conn: &mut SqliteConnection,
    games: &[&Game],
) -> Result<Vec<Value>, sqlx::Error> {
    let refs: Vec<CardRef> = games
        .iter()
        .flat_map(|game| &game.seats)
        .flat_map(|seat| commander_refs(seat.deck.as_ref()))
        .flat_map(|(id, name, printing)| [CardRef::Card(id, name), CardRef::Printing(printing)])
        .collect();
    let art = catalog::art_crop_urls_in(conn, &refs).await?;
    Ok(games
        .iter()
        .map(|game| {
            let mut seats: Vec<&Seat> = game.seats.iter().collect();
            seats.sort_by_key(|seat| (seat.result != GameResult::Win, seat.seat));
            let commanders: Vec<Value> = seats
                .iter()
                .map(|seat| {
                    let mut slots = commander_refs(seat.deck.as_ref()).into_iter();
                    let (id, name, printing) = slots.next().unwrap_or((None, None, None));
                    let mut object = Object::new();
                    object.insert("player_name".into(), json!(seat.player.name));
                    object.insert("name".into(), json!(name));
                    object.insert(
                        "game_changer".into(),
                        json!(art.game_changer(id.as_deref(), name.as_deref())),
                    );
                    object.insert(
                        "art_crop_url".into(),
                        json!(art.art_crop_url(
                            id.as_deref(),
                            name.as_deref(),
                            printing.as_deref()
                        )),
                    );
                    object.insert("winner".into(), json!(seat.result == GameResult::Win));
                    if let Some((partner_id, partner_name, partner_printing)) = slots.next() {
                        let crop = art.art_crop_url(
                            partner_id.as_deref(),
                            partner_name.as_deref(),
                            partner_printing.as_deref(),
                        );
                        let game_changer =
                            art.game_changer(partner_id.as_deref(), partner_name.as_deref());
                        object.insert("partner_name".into(), json!(partner_name));
                        object.insert("partner_art_crop_url".into(), json!(crop));
                        object.insert("partner_game_changer".into(), json!(game_changer));
                    } else {
                        object.insert("partner_name".into(), Value::Null);
                        object.insert("partner_art_crop_url".into(), Value::Null);
                        object.insert("partner_game_changer".into(), json!(false));
                    }
                    Value::Object(object)
                })
                .collect();
            let mut object = recent_game(game, &[]);
            object.insert("commanders".into(), Value::Array(commanders));
            Value::Object(object)
        })
        .collect())
}

/// `game_lengths/2`: duration (15-minute) and turn (2-turn) histograms plus the fastest
/// win and longest game. `tracked` picks a game's tracked results; `None` means any winner
/// counts as a win.
pub fn game_lengths(games: &[&Game], tracked: &dyn Fn(&Game) -> Option<Vec<GameResult>>) -> Value {
    let timed: Vec<&Game> = games
        .iter()
        .copied()
        .filter(|game| game.duration_minutes.is_some())
        .collect();
    let won = |game: &&Game| match tracked(game) {
        None => game.winner().is_some(),
        Some(results) => tracked_result(&results) == Some(GameResult::Win),
    };
    let summary =
        |game: &Game| Value::Object(recent_game(game, &tracked(game).unwrap_or_default()));
    // Enum.min_by/max_by keep the first of equal elements.
    let mut fastest: Option<&Game> = None;
    for game in timed.iter().copied().filter(won) {
        if fastest.is_none_or(|best| game.duration_minutes < best.duration_minutes) {
            fastest = Some(game);
        }
    }
    let mut longest: Option<&Game> = None;
    for game in timed.iter().copied() {
        if longest.is_none_or(|best| game.duration_minutes > best.duration_minutes) {
            longest = Some(game);
        }
    }
    json!({
        "durations": histogram(games.iter().map(|game| game.duration_minutes), 15),
        "turns": histogram(games.iter().map(|game| game.turns), 2),
        "fastest_win": fastest.map_or(Value::Null, summary),
        "longest_game": longest.map_or(Value::Null, summary),
    })
}

/// `deck/2`: a deck row for commander detail, with commander and partner crops.
pub fn deck(deck: &Deck, summaries: &CardSummaries) -> Object {
    let art = summaries.get(
        deck.commander_card_id.as_deref(),
        Some(&deck.commander_name),
    );
    let partner_art = deck
        .partner_name
        .as_deref()
        .and_then(|partner| summaries.get(deck.partner_card_id.as_deref(), Some(partner)));
    let mut object = Object::new();
    object.insert("id".into(), json!(deck.id));
    object.insert("name".into(), json!(deck.name));
    object.insert("commander_name".into(), json!(deck.commander_name));
    object.insert("color_identity".into(), json!(deck.color_identity));
    object.insert(
        "game_changer".into(),
        json!(art.is_some_and(|art| art.game_changer)),
    );
    object.insert(
        "art_crop_url".into(),
        json!(art.and_then(|art| art.art_crop_url.clone())),
    );
    object.insert(
        "partner_art_crop_url".into(),
        json!(partner_art.and_then(|art| art.art_crop_url.clone())),
    );
    object
}

/// `records/1`: adds `win_rate` and sorts.
pub fn records(counts: Vec<Object>) -> Vec<Value> {
    let mut rows: Vec<Object> = counts
        .into_iter()
        .map(|mut row| {
            let wins = row.get("wins").and_then(Value::as_u64).unwrap_or_default();
            let games = row.get("games").and_then(Value::as_u64).unwrap_or_default();
            let rate = super::records::percentage(
                usize::try_from(wins).unwrap_or_default(),
                usize::try_from(games).unwrap_or_default(),
            );
            row.insert("win_rate".into(), json!(rate));
            row
        })
        .collect();
    sort_records(&mut rows);
    rows.into_iter().map(Value::Object).collect()
}
