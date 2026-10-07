//! One player's statistics.

use super::query::DateRange;
use std::collections::HashMap;

use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::catalog::{self, ArtUrls, CardRef};
use crate::games::player::list_players;
use crate::games::{Game, GameResult, Seat, get_player};

use super::commanders::{self, SeatInGame};
use super::records::{self, Object, Record, group_by, grouped_records, seat_entity};
use super::{MIN_GAMES, elo, outcomes, query, summaries};

fn mine(game: &Game, player_id: i64) -> Option<&Seat> {
    game.seats.iter().find(|seat| seat.player_id == player_id)
}

fn results(seat: Option<&Seat>) -> Vec<GameResult> {
    seat.map(|seat| seat.result).into_iter().collect()
}

/// `Player.get/2`.
pub async fn get(
    conn: &mut SqliteConnection,
    player_id: i64,
    params: &DateRange,
) -> Result<Option<Value>, sqlx::Error> {
    let Some(player) = get_player(conn, player_id).await? else {
        return Ok(None);
    };
    let games = query::games(conn, params, Some(player.id), None).await?;
    let seats: Vec<&Seat> = games
        .iter()
        .filter_map(|game| mine(game, player.id))
        .collect();
    let oldest_first: Vec<GameResult> = seats.iter().rev().map(|seat| seat.result).collect();
    let cutoff = query::detailed_stats_from(conn).await?;
    let detailed = query::detailed(&games, cutoff);
    let detailed_seats: Vec<&Seat> = detailed
        .iter()
        .filter_map(|game| mine(game, player.id))
        .collect();
    let card_art = card_art(conn, &seats).await?;
    let with_result = |result: GameResult| {
        games
            .iter()
            .filter(move |game| mine(game, player.id).is_some_and(|seat| seat.result == result))
    };
    let decks = grouped_records(
        seats.iter().copied().filter(|seat| seat.deck.is_some()),
        |seat| {
            let mut entity = seat
                .deck
                .as_ref()
                .map(summaries::deck_entity)
                .unwrap_or_default();
            let game_changer = seat.deck.as_ref().is_some_and(|deck| {
                card_art.game_changer(
                    deck.commander_card_id.as_deref(),
                    Some(&deck.commander_name),
                )
            });
            entity.insert("game_changer".into(), json!(game_changer));
            entity
        },
        |seat| seat.deck_id,
    );
    let tracked = |game: &Game| Some(results(mine(game, player.id)));

    Ok(Some(json!({
        "detailed_stats_from": cutoff,
        "player": {"id": player.id, "name": player.name},
        "record": Record::of_seats(seats.iter().copied()).to_json(),
        "win_conditions": outcomes::win_conditions(with_result(GameResult::Win)),
        "loss_conditions": outcomes::win_conditions(with_result(GameResult::Loss)),
        "elo": player_elo(conn, player.id, params).await?,
        "average_duration_minutes": records::average(detailed.iter().map(|game| game.duration_minutes)),
        "average_turns": records::average(detailed.iter().map(|game| game.turns)),
        "game_lengths": summaries::game_lengths(&detailed, &tracked),
        "color_exposure": records::color_exposure(seats.iter().copied()),
        "rival_commanders": rival_commanders(conn, &games, player.id).await?,
        "streaks": streaks(&oldest_first),
        "recent_form": seats.iter().take(10).map(|seat| seat.result).collect::<Vec<_>>(),
        "win_rate_over_time": records::cumulative_win_rate(
            games.iter().map(|game| (game.played_at, results(mine(game, player.id)))),
        ),
        "decks": decks,
        "color_win_rates": records::color_records(seats.iter().copied()),
        "head_to_head": head_to_head(conn, &games, player.id).await?,
        "seat_win_rates": grouped_records(detailed_seats.iter().copied(), seat_entity, |seat| seat.seat),
        "favorite_seat": favorite_seat(&detailed_seats),
        "best_seat": best_seat(&detailed_seats),
        "mvp_cards": mvp_cards(&detailed_seats, &card_art),
    })))
}

/// Ratings depend on every game at the table, so the whole playgroup is replayed, including
/// games before the window. Only players with at least [`MIN_GAMES`] hold a rank.
async fn player_elo(
    conn: &mut SqliteConnection,
    player_id: i64,
    params: &DateRange,
) -> Result<Value, sqlx::Error> {
    let all = query::games(conn, &query::without_date_from(params), None, None).await?;
    let ratings = elo::ratings(&all, query::window_start(params));
    let ranked: Vec<&elo::Rating> = ratings
        .iter()
        .filter(|rating| rating.games >= MIN_GAMES)
        .collect();
    let Some(rating) = ratings.iter().find(|rating| rating.id == player_id) else {
        return Ok(Value::Null);
    };
    let rank = ranked
        .iter()
        .position(|rating| rating.id == player_id)
        .map(|index| index + 1);
    Ok(json!({
        "rating": rating.rating,
        "start": rating.start,
        "peak": rating.peak,
        "games": rating.games,
        "history": rating.history_json(),
        "rank": rank,
        "players": ranked.len(),
    }))
}

/// Opponents' commanders: how often each was faced, beat this player, or was beaten by
/// them. Mirror matches among the opponents count every seat.
async fn rival_commanders(
    conn: &mut SqliteConnection,
    games: &[Game],
    player_id: i64,
) -> Result<Vec<Value>, sqlx::Error> {
    let opponents = |won_only: bool| -> Vec<SeatInGame<'_>> {
        games
            .iter()
            .filter(|game| {
                !won_only
                    || game
                        .seats
                        .iter()
                        .any(|seat| seat.player_id == player_id && seat.result == GameResult::Win)
            })
            .flat_map(|game| {
                game.seats
                    .iter()
                    .filter(|seat| seat.player_id != player_id && seat.deck.is_some())
                    .map(move |seat| SeatInGame { seat, game })
            })
            .collect()
    };
    let faced = commanders::summarize(conn, &opponents(false)).await?;
    let beaten: HashMap<String, Value> = commanders::summarize(conn, &opponents(true))
        .await?
        .into_iter()
        .map(|row| {
            (
                row.get("id").map(Value::to_string).unwrap_or_default(),
                row.get("games").cloned().unwrap_or(json!(0)),
            )
        })
        .collect();
    let mut rows: Vec<Object> = faced
        .into_iter()
        .map(|row| {
            let mut object = Object::new();
            for key in [
                "id",
                "name",
                "image_url",
                "art_crop_url",
                "color_identity",
                "game_changer",
            ] {
                object.insert(key.into(), row.get(key).cloned().unwrap_or(Value::Null));
            }
            object.insert(
                "faced".into(),
                row.get("games").cloned().unwrap_or(json!(0)),
            );
            object.insert(
                "beat_me".into(),
                row.get("wins").cloned().unwrap_or(json!(0)),
            );
            let id = row.get("id").map(Value::to_string).unwrap_or_default();
            object.insert(
                "beaten".into(),
                beaten.get(&id).cloned().unwrap_or(json!(0)),
            );
            object
        })
        .collect();
    let number =
        |object: &Object, key: &str| object.get(key).and_then(Value::as_u64).unwrap_or_default();
    let name = |object: &Object| {
        object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase()
    };
    rows.sort_by(|a, b| {
        number(b, "faced")
            .cmp(&number(a, "faced"))
            .then(number(b, "beat_me").cmp(&number(a, "beat_me")))
            .then_with(|| name(a).cmp(&name(b)))
    });
    Ok(rows.into_iter().map(Value::Object).collect())
}

fn streaks(oldest_first: &[GameResult]) -> Value {
    let mut longest = 0;
    let mut run = 0;
    for result in oldest_first {
        if *result == GameResult::Win {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let current = oldest_first
        .iter()
        .rev()
        .take_while(|result| **result == GameResult::Win)
        .count();
    json!({"current_wins": current, "longest_wins": longest})
}

async fn head_to_head(
    conn: &mut SqliteConnection,
    games: &[Game],
    player_id: i64,
) -> Result<Vec<Value>, sqlx::Error> {
    let avatars: HashMap<i64, Option<String>> = list_players(conn, true)
        .await?
        .into_iter()
        .map(|player| (player.id, player.avatar_url))
        .collect();
    let rows = games.iter().flat_map(|game| {
        let my_result = mine(game, player_id).map(|seat| seat.result);
        game.seats
            .iter()
            .filter(|seat| seat.player_id != player_id)
            .map(move |opponent| (opponent, my_result))
    });
    let mut entries: Vec<(usize, String, Value)> =
        group_by(rows, |(opponent, _)| opponent.player_id)
            .into_values()
            .filter_map(|rows| {
                let opponent = &rows.first()?.0.player;
                let wins = rows
                    .iter()
                    .filter(|(_, mine)| *mine == Some(GameResult::Win))
                    .count();
                let losses = rows
                    .iter()
                    .filter(|(theirs, _)| theirs.result == GameResult::Win)
                    .count();
                let draws = rows
                    .iter()
                    .filter(|(_, mine)| *mine == Some(GameResult::Draw))
                    .count();
                let value = json!({
                    "id": opponent.id,
                    "name": opponent.name,
                    "avatar_url": avatars.get(&opponent.id).cloned().flatten(),
                    "games": rows.len(),
                    "wins": wins,
                    "losses": losses,
                    "draws": draws,
                });
                Some((rows.len(), opponent.name.clone(), value))
            })
            .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(entries.into_iter().map(|(_, _, value)| value).collect())
}

fn mvp_cards(seats: &[&Seat], card_art: &ArtUrls) -> Vec<Value> {
    let named = seats.iter().filter(|seat| {
        seat.mvp_card_name
            .as_deref()
            .is_some_and(|name| !name.is_empty())
    });
    let mut rows: Vec<(usize, String, Value)> = group_by(named, |seat| {
        (seat.mvp_card_id.clone(), seat.mvp_card_name.clone())
    })
    .into_iter()
    .map(|((id, name), rows)| {
        let name = name.unwrap_or_default();
        let value = json!({
            "id": id,
            "name": name,
            "mentions": rows.len(),
            "game_changer": card_art.game_changer(id.as_deref(), Some(&name)),
            "image_url": card_art.card_image_url(id.as_deref(), Some(&name), None),
            "art_crop_url": card_art.art_crop_url(id.as_deref(), Some(&name), None),
        });
        (rows.len(), name, value)
    })
    .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    rows.into_iter()
        .take(8)
        .map(|(_, _, value)| value)
        .collect()
}

async fn card_art(conn: &mut SqliteConnection, seats: &[&Seat]) -> Result<ArtUrls, sqlx::Error> {
    let refs: Vec<CardRef> = seats
        .iter()
        .flat_map(|seat| {
            let mut refs = vec![CardRef::Card(
                seat.mvp_card_id.clone(),
                seat.mvp_card_name.clone(),
            )];
            if let Some(deck) = &seat.deck {
                refs.push(CardRef::Card(
                    deck.commander_card_id.clone(),
                    Some(deck.commander_name.clone()),
                ));
                refs.push(CardRef::Card(
                    deck.partner_card_id.clone(),
                    deck.partner_name.clone(),
                ));
            }
            refs
        })
        .collect();
    catalog::art_crop_urls_in(conn, &refs).await
}

fn favorite_seat(seats: &[&Seat]) -> Option<i64> {
    let mut best: Option<(usize, i64)> = None;
    for (seat, rows) in group_by(seats.iter(), |seat| seat.seat) {
        if best.is_none_or(|(count, _)| rows.len() > count) {
            best = Some((rows.len(), seat));
        }
    }
    best.map(|(_, seat)| seat)
}

fn best_seat(seats: &[&Seat]) -> Option<i64> {
    let mut best: Option<((f64, usize), i64)> = None;
    for (seat, rows) in group_by(seats.iter(), |seat| seat.seat) {
        let score = (
            Record::of_seats(rows.iter().map(|seat| **seat)).win_rate,
            rows.len(),
        );
        let better = best.is_none_or(|((rate, count), _)| {
            score.0.total_cmp(&rate).then(score.1.cmp(&count)) == std::cmp::Ordering::Greater
        });
        if better {
            best = Some((score, seat));
        }
    }
    best.map(|(_, seat)| seat)
}
