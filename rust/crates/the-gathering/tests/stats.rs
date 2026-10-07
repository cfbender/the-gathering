//! Statistics views, Elo ratings, and records.
// Test crates: helpers outside `#[test]` functions may unwrap and index freely, like the
// tests themselves (clippy.toml only exempts `#[test]` bodies).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::assert_is_empty
)]

mod support;

use std::collections::HashMap;

use serde_json::{Value, json};
use support::{TestApp, utc};
use the_gathering::games::{Deck, Game, GameResult, Player, Seat};
use the_gathering::stats::{self, elo, records};

/// `(winner, win condition, kills per seat)`.
type OutcomeRow = (Option<&'static str>, Option<&'static str>, [Option<i64>; 3]);

struct Fixture {
    app: TestApp,
    players: HashMap<&'static str, Player>,
    decks: HashMap<&'static str, Deck>,
}

impl Fixture {
    fn id(&self, name: &str) -> i64 {
        self.players[name].id
    }

    /// `game/5`: everyone plays their deck; Alice's wins name Swords to Plowshares MVP.
    async fn game(
        &self,
        decks: &HashMap<&'static str, Deck>,
        played_at: &str,
        winner: &str,
        order: &[&str],
    ) {
        let seats: Vec<Value> = order
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let alice_won = *name == "Alice" && winner == "Alice";
                json!({
                    "player_id": self.id(name),
                    "deck_id": decks[name].id,
                    "seat": index + 1,
                    "result": if *name == winner { "win" } else { "loss" },
                    "mvp_card_id": if alice_won { Some("swords") } else { None },
                    "mvp_card_name": if alice_won { Some("Swords to Plowshares") } else { None },
                })
            })
            .collect();
        self.app
            .game(
                json!({"played_at": played_at, "duration_minutes": 75, "turns": 9, "source": "manual", "seats": seats}),
                None,
            )
            .await;
    }

    async fn draw(&self, played_at: &str) {
        let seats: Vec<Value> = ["Alice", "Bob", "Cara"]
            .iter()
            .enumerate()
            .map(|(index, name)| {
                json!({"player_id": self.id(name), "deck_id": self.decks[name].id, "seat": index + 1, "result": "draw"})
            })
            .collect();
        self.app
            .game(
                json!({"played_at": played_at, "source": "manual", "seats": seats}),
                None,
            )
            .await;
    }

    async fn overview(&self, params: Value) -> Value {
        stats::overview(self.app.pool(), &params).await.unwrap()
    }

    async fn player(&self, name: &str, params: Value) -> Value {
        stats::player(self.app.pool(), self.id(name), &params)
            .await
            .unwrap()
            .unwrap()
    }

    async fn commander(&self, id: &str, params: Value) -> Option<Value> {
        stats::commander(self.app.pool(), id, &params)
            .await
            .unwrap()
    }

    async fn commanders(&self, params: Value) -> Vec<Value> {
        stats::commanders(self.app.pool(), &params).await.unwrap()
    }
}

async fn setup() -> Fixture {
    let app = TestApp::new().await;
    app.card(
        "kangee",
        "Kangee, Sky Warden",
        &[],
        json!({"art_crop": "https://cards.example/kangee-art.jpg"}),
        true,
    )
    .await;
    app.card(
        "swords",
        "Swords to Plowshares",
        &[],
        json!({"art_crop": "https://cards.example/swords-art.jpg"}),
        false,
    )
    .await;
    let mut players = HashMap::new();
    for name in ["Alice", "Bob", "Cara"] {
        players.insert(name, app.player(name).await);
    }
    let birds = app
        .deck_with(json!({
            "player_id": players["Alice"].id, "name": "Birds", "commander_card_id": "kangee",
            "commander_name": "Kangee, Sky Warden", "color_identity": "WU",
        }))
        .await;
    let goblins = app
        .deck_with(json!({"player_id": players["Bob"].id, "name": "Goblins", "commander_name": "Krenko, Mob Boss", "color_identity": "R"}))
        .await;
    let elves = app
        .deck_with(json!({
            "player_id": players["Cara"].id, "name": "Elves", "commander_name": "Lathril, Blade of the Elves", "color_identity": "BG",
        }))
        .await;
    let decks = HashMap::from([("Alice", birds), ("Bob", goblins), ("Cara", elves)]);
    let fixture = Fixture {
        app,
        players,
        decks,
    };
    let decks = fixture.decks.clone();
    fixture
        .game(
            &decks,
            "2026-01-01T00:00:00Z",
            "Alice",
            &["Alice", "Bob", "Cara"],
        )
        .await;
    fixture
        .game(
            &decks,
            "2026-01-15T12:00:00Z",
            "Bob",
            &["Bob", "Cara", "Alice"],
        )
        .await;
    fixture
        .game(
            &decks,
            "2026-02-01T23:59:59Z",
            "Alice",
            &["Cara", "Alice", "Bob"],
        )
        .await;
    fixture
        .game(
            &decks,
            "2026-02-12T12:00:00Z",
            "Alice",
            &["Alice", "Cara", "Bob"],
        )
        .await;
    fixture
        .game(
            &decks,
            "2026-03-01T00:00:00Z",
            "Cara",
            &["Bob", "Alice", "Cara"],
        )
        .await;
    fixture.draw("2026-03-05T12:00:00Z").await;
    fixture
}

#[allow(clippy::needless_pass_by_value)] // Call sites build `json!` values inline.
fn find<'a>(rows: &'a Value, key: &str, value: Value) -> &'a Value {
    rows.as_array()
        .and_then(|rows| rows.iter().find(|row| row[key] == value))
        .unwrap_or_else(|| panic!("no row with {key} = {value} in {rows}"))
}

fn pairs(rows: &Value, keys: &[&str]) -> Vec<Vec<Value>> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| keys.iter().map(|key| row[*key].clone()).collect())
        .collect()
}

#[tokio::test]
async fn date_ranges_follow_the_requested_time_zones_calendar_days() {
    let f = setup().await;
    let range = json!({"date_from": "2026-01-16", "date_to": "2026-01-16"});
    assert_eq!(f.overview(range.clone()).await["games_count"], 0);
    let mut kiritimati = range.clone();
    kiritimati["tz"] = json!("Pacific/Kiritimati");
    assert_eq!(f.overview(kiritimati).await["games_count"], 1);
    let mut bogus = range;
    bogus["tz"] = json!("Not/AZone");
    assert_eq!(f.overview(bogus).await["games_count"], 0);
}

#[tokio::test]
async fn overview_reports_asymmetric_records_draws_seats_colors_and_inclusive_boundaries() {
    let f = setup().await;
    let stats = f
        .overview(json!({"date_from": "2026-01-01", "date_to": "2026-02-01"}))
        .await;
    assert_eq!(stats["games_count"], 3);
    assert_eq!(
        stats["games_by_month"],
        json!([{"month": "2026-01", "games": 2}, {"month": "2026-02", "games": 1}])
    );
    assert_eq!(
        find(&stats["leaderboard"], "id", json!(f.id("Alice"))),
        &json!({"id": f.id("Alice"), "name": "Alice", "games": 3, "wins": 2, "losses": 1, "draws": 0, "win_rate": 66.7})
    );
    let seat_two = find(&stats["seat_win_rates"], "id", json!(2));
    assert_eq!(
        (
            seat_two["games"].clone(),
            seat_two["wins"].clone(),
            seat_two["win_rate"].clone()
        ),
        (json!(3), json!(1), json!(33.3))
    );
    let wu = find(&stats["color_win_rates"], "id", json!("WU"));
    assert_eq!(wu["name"], "Azorius");
    assert_eq!(
        (
            wu["games"].clone(),
            wu["wins"].clone(),
            wu["win_rate"].clone()
        ),
        (json!(3), json!(2), json!(66.7))
    );

    let all = f.overview(json!({})).await;
    assert_eq!(
        pairs(&all["color_exposure"], &["id", "games"]),
        ["W", "U", "B", "R", "G"].map(|id| vec![json!(id), json!(6)])
    );
    let kangee = find(&stats["commanders"], "id", json!("kangee"));
    assert_eq!(kangee["name"], "Kangee, Sky Warden");
    assert_eq!(
        kangee["art_crop_url"],
        "https://cards.example/kangee-art.jpg"
    );
    assert_eq!(
        (
            kangee["games"].clone(),
            kangee["wins"].clone(),
            kangee["losses"].clone()
        ),
        (json!(3), json!(2), json!(1))
    );
}

#[tokio::test]
async fn recent_games_include_one_winner_first_portrait_per_seat_carrying_partner_art_with_catalog_name_fallback()
 {
    let f = setup().await;
    f.app
        .state
        .games
        .update_deck(
            &f.decks["Bob"],
            &json!({"partner_name": "Kangee, Sky Warden"}),
        )
        .await
        .unwrap();
    let stats = f
        .overview(json!({"date_from": "2026-01-15", "date_to": "2026-01-15"}))
        .await;
    let recent = stats["recent_games"].as_array().unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0]["players"], 3);
    let commanders = recent[0]["commanders"].as_array().unwrap();
    let (bob, cara, alice) = (&commanders[0], &commanders[1], &commanders[2]);
    assert_eq!(bob["player_name"], "Bob");
    assert_eq!(bob["name"], "Krenko, Mob Boss");
    assert_eq!(bob["winner"], true);
    assert_eq!(bob["art_crop_url"], Value::Null);
    assert_eq!(bob["partner_name"], "Kangee, Sky Warden");
    assert_eq!(
        bob["partner_art_crop_url"],
        "https://cards.example/kangee-art.jpg"
    );
    assert_eq!(
        (
            cara["player_name"].clone(),
            cara["winner"].clone(),
            cara["partner_name"].clone()
        ),
        (json!("Cara"), json!(false), Value::Null)
    );
    assert_eq!(alice["player_name"], "Alice");
    assert_eq!(alice["winner"], false);
    assert_eq!(
        alice["art_crop_url"],
        "https://cards.example/kangee-art.jpg"
    );
    assert_eq!(alice["partner_art_crop_url"], Value::Null);

    let all = f.overview(json!({})).await;
    assert_eq!(all["recent_games"][0]["winner"], Value::Null);
    assert!(
        all["recent_games"][0]["commanders"]
            .as_array()
            .unwrap()
            .iter()
            .all(|portrait| portrait["winner"] == false)
    );
}

#[tokio::test]
async fn outcomes_include_historical_data_retain_zeros_and_count_only_a_players_own_wins() {
    let f = setup().await;
    f.app
        .settings(json!({"detailed_stats_from": "2027-01-01"}))
        .await;
    let rows: [OutcomeRow; 6] = [
        (
            Some("Alice"),
            Some("combat_damage"),
            [Some(2), Some(0), None],
        ),
        (
            Some("Bob"),
            Some("combat_damage"),
            [Some(1), Some(1), Some(0)],
        ),
        (Some("Alice"), Some("infinite_combo"), [Some(0), None, None]),
        (Some("Alice"), Some("unknown"), [None, None, None]),
        (None, Some("draw"), [None, None, None]),
        (Some("Alice"), None, [None, None, None]),
    ];
    for (day, (winner, condition, kills)) in rows.iter().enumerate() {
        let seats: Vec<Value> = ["Alice", "Bob", "Cara"]
            .iter()
            .zip(kills)
            .enumerate()
            .map(|(index, (name, kills))| {
                let result = match winner {
                    None => "draw",
                    Some(winner) if winner == name => "win",
                    Some(_) => "loss",
                };
                json!({"player_id": f.id(name), "seat": index + 1, "kills": kills, "result": result})
            })
            .collect();
        f.app
            .game(json!({"played_at": format!("2026-04-0{}T00:00:00Z", day + 1), "win_condition": condition, "seats": seats}), None)
            .await;
    }
    let range = json!({"date_from": "2026-04-01", "date_to": "2026-04-06"});
    let stats = f.overview(range.clone()).await;
    assert!(
        stats["recent_games"][0]["commanders"]
            .as_array()
            .unwrap()
            .iter()
            .all(|portrait| portrait["name"].is_null() && portrait["art_crop_url"].is_null())
    );
    assert_eq!(stats["kills"]["total"], 4);
    assert_eq!(stats["kills"]["recorded_seats"], 6);
    assert_eq!(stats["kills"]["total_seats"], 18);
    assert_eq!(
        pairs(
            &stats["kills"]["players"],
            &["name", "kills", "recorded_games", "average"]
        ),
        vec![
            vec![json!("Alice"), json!(3), json!(3), json!(1.0)],
            vec![json!("Bob"), json!(1), json!(2), json!(0.5)],
            vec![json!("Cara"), json!(0), json!(1), json!(0.0)],
        ]
    );
    assert_eq!(
        stats["win_conditions"],
        json!({"total_games": 6, "recorded_games": 4, "conditions": [
            {"condition": "combat_damage", "games": 2},
            {"condition": "draw", "games": 1},
            {"condition": "infinite_combo", "games": 1},
        ]})
    );
    assert_eq!(
        f.player("Alice", range.clone()).await["win_conditions"],
        json!({"total_games": 4, "recorded_games": 2, "conditions": [
            {"condition": "combat_damage", "games": 1},
            {"condition": "infinite_combo", "games": 1},
        ]})
    );
    let narrow = json!({"date_from": "2026-04-02", "date_to": "2026-04-03"});
    assert_eq!(f.overview(narrow.clone()).await["kills"]["total"], 2);
    assert_eq!(
        f.overview(narrow.clone()).await["win_conditions"]["recorded_games"],
        2
    );
    assert_eq!(
        f.player("Alice", range.clone()).await["loss_conditions"],
        json!({"total_games": 1, "recorded_games": 1, "conditions": [{"condition": "combat_damage", "games": 1}]})
    );
    assert_eq!(
        f.player("Cara", range.clone()).await["loss_conditions"],
        json!({"total_games": 5, "recorded_games": 3, "conditions": [
            {"condition": "combat_damage", "games": 2},
            {"condition": "infinite_combo", "games": 1},
        ]})
    );
    assert_eq!(
        f.player("Cara", narrow.clone()).await["loss_conditions"]["total_games"],
        2
    );
    assert_eq!(
        f.player("Alice", narrow).await["win_conditions"]["conditions"],
        json!([{"condition": "infinite_combo", "games": 1}])
    );
}

#[tokio::test]
async fn missing_outcomes_remain_empty_instead_of_becoming_zero_counts_or_favorites() {
    let f = setup().await;
    let all = f.overview(json!({})).await;
    assert_eq!(
        all["kills"],
        json!({"total": 0, "recorded_seats": 0, "total_seats": 18, "players": []})
    );
    assert_eq!(
        all["win_conditions"],
        json!({"total_games": 6, "recorded_games": 0, "conditions": []})
    );
    assert_eq!(
        f.player("Alice", json!({})).await["win_conditions"],
        json!({"total_games": 3, "recorded_games": 0, "conditions": []})
    );
    let empty = f.overview(json!({"date_from": "2027-01-01"})).await;
    assert_eq!(
        empty["kills"],
        json!({"total": 0, "recorded_seats": 0, "total_seats": 0, "players": []})
    );
    assert_eq!(
        empty["win_conditions"],
        json!({"total_games": 0, "recorded_games": 0, "conditions": []})
    );
}

#[tokio::test]
async fn player_stats_compute_ordered_current_and_longest_streaks_plus_head_to_head() {
    let f = setup().await;
    let stats = f.player("Alice", json!({})).await;
    assert_eq!(
        stats["record"],
        json!({"games": 6, "wins": 3, "losses": 2, "draws": 1, "win_rate": 50.0})
    );
    assert_eq!(
        stats["streaks"],
        json!({"current_wins": 0, "longest_wins": 2})
    );
    assert_eq!(
        stats["recent_form"],
        json!(["draw", "loss", "win", "win", "loss", "win"])
    );
    assert_eq!(stats["favorite_seat"], 1);
    assert_eq!(stats["best_seat"], 1);
    let bob = find(&stats["head_to_head"], "id", json!(f.id("Bob")));
    assert_eq!(
        pairs(&json!([bob]), &["games", "wins", "losses", "draws"]),
        vec![vec![json!(6), json!(3), json!(1), json!(1)]]
    );
}

#[tokio::test]
async fn rival_players_carry_their_linked_users_avatar() {
    let f = setup().await;
    let user = f.app.unique_member().await;
    sqlx::query("UPDATE users SET avatar_url = 'https://cdn/bob.png' WHERE id = ?")
        .bind(user.id)
        .execute(f.app.pool())
        .await
        .unwrap();
    f.app
        .state
        .games
        .link_player_to_user(&f.players["Bob"], &user)
        .await
        .unwrap();
    let stats = f.player("Alice", json!({})).await;
    assert_eq!(
        find(&stats["head_to_head"], "id", json!(f.id("Bob")))["avatar_url"],
        "https://cdn/bob.png"
    );
    assert_eq!(
        find(&stats["head_to_head"], "id", json!(f.id("Cara")))["avatar_url"],
        Value::Null
    );
    let kangee = f.commander("kangee", json!({})).await.unwrap();
    assert_eq!(
        find(&kangee["opponents"], "name", json!("Bob"))["avatar_url"],
        "https://cdn/bob.png"
    );
}

#[tokio::test]
async fn player_color_records_count_only_that_players_seats_and_merge_decks_by_color() {
    let f = setup().await;
    let alice = f.id("Alice");
    let more_birds = f
        .app
        .deck_with(json!({"player_id": alice, "name": "More Birds", "commander_name": "Isperia, Supreme Judge", "color_identity": "UW"}))
        .await;
    let rats = f
        .app
        .deck_with(json!({"player_id": alice, "name": "Rats", "commander_name": "Marrow-Gnawer", "color_identity": "B"}))
        .await;
    let mut with_more = f.decks.clone();
    with_more.insert("Alice", more_birds.clone());
    let mut with_rats = f.decks.clone();
    with_rats.insert("Alice", rats.clone());
    f.game(
        &with_more,
        "2026-04-01T00:00:00Z",
        "Bob",
        &["Alice", "Bob", "Cara"],
    )
    .await;
    f.game(
        &with_rats,
        "2026-04-02T00:00:00Z",
        "Alice",
        &["Alice", "Bob", "Cara"],
    )
    .await;
    f.game(
        &with_rats,
        "2026-04-03T00:00:00Z",
        "Cara",
        &["Alice", "Bob", "Cara"],
    )
    .await;

    let stats = f.player("Alice", json!({})).await;
    assert_eq!(
        pairs(
            &stats["color_win_rates"],
            &["id", "name", "games", "wins", "win_rate"]
        ),
        vec![
            vec![
                json!("WU"),
                json!("Azorius"),
                json!(7),
                json!(3),
                json!(42.9)
            ],
            vec![
                json!("B"),
                json!("Mono-Black"),
                json!(2),
                json!(1),
                json!(50.0)
            ],
        ]
    );
    assert!(
        !stats["color_win_rates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == "R")
    );
    assert_eq!(
        find(
            &f.overview(json!({})).await["color_win_rates"],
            "id",
            json!("R")
        )["games"],
        9
    );

    f.app
        .state
        .games
        .update_deck(&rats, &json!({"archived_at": "2026-05-01T00:00:00Z"}))
        .await
        .unwrap();
    let retired = f.player("Alice", json!({})).await;
    assert_eq!(retired["record"], stats["record"]);
    let rats_row = find(&retired["decks"], "id", json!(rats.id));
    assert_eq!(
        (
            rats_row["retired"].clone(),
            rats_row["games"].clone(),
            rats_row["wins"].clone()
        ),
        (json!(true), json!(2), json!(1))
    );
    assert!(
        find(&retired["decks"], "id", json!(more_birds.id))
            .get("retired")
            .is_none()
    );
    assert!(
        f.overview(json!({})).await["leaderboard"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("retired").is_none())
    );
}

#[tokio::test]
async fn overview_and_player_views_add_elo_matchups_game_lengths_colors_and_rivals() {
    let f = setup().await;
    let seats: Vec<Value> = ["Bob", "Alice", "Cara"]
        .iter()
        .enumerate()
        .map(|(index, name)| {
            json!({"player_id": f.id(name), "deck_id": f.decks[name].id, "seat": index + 1,
                   "result": if *name == "Bob" { "win" } else { "loss" }})
        })
        .collect();
    f.app
        .game(json!({"played_at": "2026-04-01T12:00:00Z", "duration_minutes": 40, "turns": 6, "source": "manual", "seats": seats}), None)
        .await;

    let overview = f.overview(json!({})).await;
    assert_eq!(overview["game_times"].as_array().unwrap().len(), 7);
    assert_eq!(overview["game_times"][0], "2026-04-01T12:00:00Z");
    assert_eq!(
        overview["game_lengths"]["durations"],
        json!([
            {"from": 30, "to": 45, "games": 1},
            {"from": 45, "to": 60, "games": 0},
            {"from": 60, "to": 75, "games": 0},
            {"from": 75, "to": 90, "games": 5},
        ])
    );
    assert_eq!(
        overview["game_lengths"]["turns"],
        json!([{"from": 6, "to": 8, "games": 1}, {"from": 8, "to": 10, "games": 5}])
    );
    let fastest = &overview["game_lengths"]["fastest_win"];
    assert_eq!(
        (
            fastest["duration_minutes"].clone(),
            fastest["winner"]["name"].clone(),
            fastest["result"].clone()
        ),
        (json!(40), json!("Bob"), Value::Null)
    );
    assert_eq!(
        overview["game_lengths"]["longest_game"]["duration_minutes"],
        75
    );

    let elo = overview["elo"].as_array().unwrap();
    assert_eq!(
        (elo[0]["name"].clone(), elo[0]["games"].clone()),
        (json!("Alice"), json!(7))
    );
    assert_eq!(elo[0]["history"].as_array().unwrap().len(), 7);
    let total: i64 = elo
        .iter()
        .map(|rating| rating["rating"].as_i64().unwrap())
        .sum();
    assert!((total - 3000).abs() <= 1);

    let alice_bob = overview["matchups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == f.id("Alice") && row["opponent_id"] == f.id("Bob"))
        .unwrap();
    assert_eq!(
        pairs(
            &json!([alice_bob]),
            &["games", "wins", "losses", "draws", "win_rate"]
        )[0],
        [json!(7), json!(3), json!(3), json!(1), json!(42.9)]
    );
    let bob_alice = overview["matchups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == f.id("Bob") && row["opponent_id"] == f.id("Alice"))
        .unwrap();
    assert_eq!(
        pairs(&json!([bob_alice]), &["games", "wins", "win_rate"])[0],
        [json!(7), json!(2), json!(28.6)]
    );

    let player = f.player("Alice", json!({})).await;
    assert_eq!(
        pairs(&json!([player["elo"]]), &["rank", "players", "games"])[0],
        [json!(1), json!(3), json!(7)]
    );
    assert_eq!(player["elo"]["history"].as_array().unwrap().len(), 7);
    assert_eq!(player["average_duration_minutes"], 69.2);
    assert_eq!(player["average_turns"], 8.5);
    assert_eq!(
        player["game_lengths"]["fastest_win"]["duration_minutes"],
        75
    );
    assert_eq!(player["game_lengths"]["fastest_win"]["result"], "win");
    assert_eq!(
        player["game_lengths"]["longest_game"]["duration_minutes"],
        75
    );
    assert_eq!(
        pairs(&player["color_exposure"], &["id", "games", "wins", "share"]),
        vec![
            vec![json!("W"), json!(7), json!(3), json!(100.0)],
            vec![json!("U"), json!(7), json!(3), json!(100.0)],
            vec![json!("B"), json!(0), json!(0), json!(0.0)],
            vec![json!("R"), json!(0), json!(0), json!(0.0)],
            vec![json!("G"), json!(0), json!(0), json!(0.0)],
        ]
    );
    assert_eq!(
        pairs(
            &player["rival_commanders"],
            &["name", "faced", "beat_me", "beaten"]
        ),
        vec![
            vec![json!("Krenko, Mob Boss"), json!(7), json!(2), json!(3)],
            vec![
                json!("Lathril, Blade of the Elves"),
                json!(7),
                json!(1),
                json!(3)
            ],
        ]
    );
    let kangee = f.commander("kangee", json!({})).await.unwrap();
    let bob = find(&kangee["opponents"], "name", json!("Bob"));
    assert_eq!(
        pairs(&json!([bob]), &["games", "wins", "beaten"])[0],
        [json!(7), json!(2), json!(3)]
    );
}

#[tokio::test]
async fn a_date_range_carries_elo_in_from_earlier_games() {
    let f = setup().await;
    let params = json!({"date_from": "2026-02-01"});
    let full = f.overview(json!({})).await["elo"].clone();
    let windowed = f.overview(params.clone()).await["elo"].clone();
    assert_eq!(
        pairs(&windowed, &["id", "rating"]),
        pairs(&full, &["id", "rating"])
    );
    assert!(
        windowed
            .as_array()
            .unwrap()
            .iter()
            .any(|rating| rating["start"] != 1000)
    );
    for rating in windowed.as_array().unwrap() {
        assert_eq!(
            rating["history"][0],
            json!({"date": "2026-02-01", "rating": rating["start"]})
        );
    }
    let alice = f.player("Alice", params).await["elo"].clone();
    assert_eq!(
        alice["rating"],
        find(&full, "id", json!(f.id("Alice")))["rating"]
    );
    assert_eq!(alice["history"][0]["date"], "2026-02-01");
}

#[tokio::test]
async fn players_below_the_game_floor_are_rated_but_unranked() {
    let f = setup().await;
    let dana = f.app.player("Dana").await;
    for played_at in ["2026-04-02T12:00:00Z", "2026-04-03T12:00:00Z"] {
        f.app
            .game(
                json!({"played_at": played_at, "source": "manual", "seats": [
                    {"player_id": dana.id, "seat": 1, "result": "win"},
                    {"player_id": f.id("Alice"), "deck_id": f.decks["Alice"].id, "seat": 2, "result": "loss"},
                    {"player_id": f.id("Bob"), "deck_id": f.decks["Bob"].id, "seat": 3, "result": "loss"},
                ]}),
                None,
            )
            .await;
    }
    assert_eq!(stats::MIN_GAMES, 3);
    assert_eq!(f.overview(json!({})).await["elo"][0]["name"], "Dana");
    let dana_elo = stats::player(f.app.pool(), dana.id, &json!({}))
        .await
        .unwrap()
        .unwrap()["elo"]
        .clone();
    assert_eq!(
        pairs(&json!([dana_elo]), &["rank", "players", "games"])[0],
        [Value::Null, json!(3), json!(2)]
    );
    let alice = f.player("Alice", json!({})).await["elo"].clone();
    assert_eq!(
        (alice["players"].clone(), alice["games"].clone()),
        (json!(3), json!(8))
    );
    assert!((1..=3).contains(&alice["rank"].as_i64().unwrap()));
}

#[tokio::test]
async fn deck_stats_include_record_opponents_averages_and_recent_results() {
    let f = setup().await;
    let stats = stats::deck(f.app.pool(), f.decks["Alice"].id, &json!({}))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stats["record"],
        json!({"games": 6, "wins": 3, "losses": 2, "draws": 1, "win_rate": 50.0})
    );
    assert_eq!(stats["average_duration_minutes"], 75.0);
    assert_eq!(stats["average_turns"], 9.0);
    assert_eq!(
        stats["recent_games"]
            .as_array()
            .unwrap()
            .iter()
            .map(|game| game["result"].clone())
            .collect::<Vec<_>>(),
        ["draw", "loss", "win", "win", "loss", "win"].map(|result| json!(result))
    );
    assert!(
        stats["opponents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "Bob" && row["games"] == 6)
    );
}

#[tokio::test]
async fn the_detailed_stats_cutoff_keeps_records_but_drops_earlier_seat_timing_and_mvp_data() {
    let f = setup().await;
    f.app
        .settings(json!({"detailed_stats_from": "2026-02-12"}))
        .await;
    let overview = f.overview(json!({})).await;
    assert_eq!(overview["detailed_stats_from"], "2026-02-12");
    assert_eq!(overview["games_count"], 6);
    let alice = find(&overview["leaderboard"], "id", json!(f.id("Alice")));
    assert_eq!(
        (alice["games"].clone(), alice["wins"].clone()),
        (json!(6), json!(3))
    );
    let seat_one = find(&overview["seat_win_rates"], "id", json!(1));
    assert_eq!(
        pairs(&json!([seat_one]), &["games", "wins", "win_rate"])[0],
        [json!(3), json!(1), json!(33.3)]
    );

    let player = f.player("Alice", json!({})).await;
    assert_eq!(
        player["record"],
        json!({"games": 6, "wins": 3, "losses": 2, "draws": 1, "win_rate": 50.0})
    );
    assert_eq!(
        player["streaks"],
        json!({"current_wins": 0, "longest_wins": 2})
    );
    let seat_one = find(&player["seat_win_rates"], "id", json!(1));
    assert_eq!(
        (seat_one["games"].clone(), seat_one["wins"].clone()),
        (json!(2), json!(1))
    );
    assert_eq!(player["favorite_seat"], 1);
    let mvps = player["mvp_cards"].as_array().unwrap();
    assert_eq!(mvps.len(), 1);
    assert_eq!(mvps[0]["name"], "Swords to Plowshares");
    assert_eq!(mvps[0]["mentions"], 1);
    assert_eq!(
        mvps[0]["art_crop_url"],
        "https://cards.example/swords-art.jpg"
    );

    f.app
        .settings(json!({"detailed_stats_from": "2026-03-05"}))
        .await;
    let deck = stats::deck(f.app.pool(), f.decks["Alice"].id, &json!({}))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(deck["record"]["games"], 6);
    assert_eq!(deck["average_duration_minutes"], Value::Null);
    assert_eq!(deck["average_turns"], Value::Null);
    assert_eq!(f.player("Alice", json!({})).await["mvp_cards"], json!([]));

    f.app.settings(json!({"detailed_stats_from": ""})).await;
    assert_eq!(
        f.overview(json!({})).await["detailed_stats_from"],
        Value::Null
    );
    let deck = stats::deck(f.app.pool(), f.decks["Alice"].id, &json!({}))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(deck["average_turns"], 9.0);
}

#[tokio::test]
async fn commander_stats_aggregate_across_pilots_count_partner_decks_for_both_partners_and_fall_back_to_names()
 {
    let f = setup().await;
    let partners = f
        .app
        .deck_with(json!({
            "player_id": f.id("Bob"), "name": "Partners", "commander_name": "Krenko, Mob Boss",
            "partner_card_id": "kangee", "partner_name": "Kangee, Sky Warden", "color_identity": "WUR",
        }))
        .await;
    let mut decks = f.decks.clone();
    decks.insert("Bob", partners);
    f.game(
        &decks,
        "2026-04-01T12:00:00Z",
        "Bob",
        &["Bob", "Alice", "Cara"],
    )
    .await;

    let commanders = f.commanders(json!({})).await;
    let mut names: Vec<&str> = commanders
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "Kangee, Sky Warden",
            "Krenko, Mob Boss",
            "Lathril, Blade of the Elves"
        ]
    );
    let rows = Value::Array(commanders.clone());
    let kangee = find(&rows, "id", json!("kangee"));
    for (key, value) in [
        ("name", json!("Kangee, Sky Warden")),
        (
            "art_crop_url",
            json!("https://cards.example/kangee-art.jpg"),
        ),
        ("games", json!(8)),
        ("wins", json!(4)),
        ("losses", json!(3)),
        ("draws", json!(1)),
        ("win_rate", json!(50.0)),
        ("pilots", json!(2)),
        ("decks", json!(2)),
        ("last_played_at", json!("2026-04-01T12:00:00Z")),
    ] {
        assert_eq!(kangee[key], value, "{key}");
    }
    let krenko = find(&rows, "name", json!("Krenko, Mob Boss"));
    assert_eq!(
        pairs(
            &json!([krenko]),
            &["id", "art_crop_url", "games", "wins", "pilots", "decks"]
        )[0],
        [
            json!("Krenko, Mob Boss"),
            Value::Null,
            json!(7),
            json!(2),
            json!(1),
            json!(2)
        ]
    );

    let detail = f.commander("kangee", json!({})).await.unwrap();
    assert_eq!(detail["commander"]["id"], "kangee");
    assert_eq!(
        detail["record"],
        json!({"games": 8, "wins": 4, "losses": 3, "draws": 1, "win_rate": 50.0})
    );
    assert_eq!(
        pairs(&detail["pilots"], &["name", "games"]),
        vec![vec![json!("Alice"), json!(7)], vec![json!("Bob"), json!(1)]]
    );
    assert_eq!(
        pairs(&detail["decks"], &["name", "games"]),
        vec![
            vec![json!("Birds"), json!(7)],
            vec![json!("Partners"), json!(1)]
        ]
    );
    assert_eq!(
        pairs(&detail["partners"], &["name", "games", "wins"]),
        vec![vec![json!("Krenko, Mob Boss"), json!(1), json!(1)]]
    );
    assert!(
        detail["opponents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "Cara" && row["games"] == 7)
    );
    let trend = detail["win_rate_over_time"].as_array().unwrap();
    assert_eq!(trend.len(), 7);
    assert_eq!(trend.last().unwrap()["win_rate"], 50.0);
    assert_eq!(detail["recent_games"][0]["result"], "win");

    let by_name = f.commander("krenko, mob boss", json!({})).await.unwrap();
    assert_eq!(by_name["record"]["games"], 7);
    assert_eq!(
        pairs(&by_name["partners"], &["name", "games"]),
        vec![vec![json!("Kangee, Sky Warden"), json!(1)]]
    );
    let partner_row = find(&by_name["decks"], "name", json!("Partners"));
    assert_eq!(partner_row["art_crop_url"], Value::Null);
    assert_eq!(
        partner_row["partner_art_crop_url"],
        "https://cards.example/kangee-art.jpg"
    );
    let other = by_name["decks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] != "Partners")
        .unwrap();
    assert_eq!(other["partner_art_crop_url"], Value::Null);

    assert_eq!(
        f.commander("kangee", json!({"date_from": "2026-04-01"}))
            .await
            .unwrap()["record"]["games"],
        2
    );
    assert!(
        f.commander("00000000-0000-0000-0000-000000000000", json!({}))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn commander_art_comes_from_the_commanders_most_played_deck() {
    let f = setup().await;
    for id in ["kangee-alt", "kangee-promo"] {
        f.app
            .printing(
                id,
                "kangee",
                "Kangee, Sky Warden",
                json!({"art_crop": format!("https://cards.example/{id}-art.jpg"), "normal": format!("https://cards.example/{id}-card.jpg")}),
            )
            .await;
    }
    let promo = f
        .app
        .deck_with(json!({
            "player_id": f.id("Bob"), "name": "Promo Kangee", "commander_card_id": "kangee",
            "commander_name": "Kangee, Sky Warden", "commander_printing_id": "kangee-promo", "color_identity": "WU",
        }))
        .await;
    let mut decks = f.decks.clone();
    decks.insert("Bob", promo);
    f.game(&decks, "2026-04-01T12:00:00Z", "Bob", &["Bob", "Cara"])
        .await;

    let art = |rows: Vec<Value>| {
        let kangee = rows.into_iter().find(|row| row["id"] == "kangee").unwrap();
        (kangee["art_crop_url"].clone(), kangee["image_url"].clone())
    };
    assert_eq!(
        art(f.commanders(json!({})).await).0,
        "https://cards.example/kangee-art.jpg"
    );
    assert_eq!(
        art(f.commanders(json!({"date_from": "2026-04-01"})).await),
        (
            json!("https://cards.example/kangee-promo-art.jpg"),
            json!("https://cards.example/kangee-promo-card.jpg")
        )
    );
    f.app
        .state
        .games
        .update_deck(
            &f.decks["Alice"],
            &json!({"commander_printing_id": "kangee-alt"}),
        )
        .await
        .unwrap();
    let alt = json!("https://cards.example/kangee-alt-art.jpg");
    assert_eq!(art(f.commanders(json!({})).await).0, alt);
    assert_eq!(
        f.commander("kangee", json!({})).await.unwrap()["commander"]["art_crop_url"],
        alt
    );
    assert_eq!(
        find(
            &f.overview(json!({})).await["commanders"],
            "id",
            json!("kangee")
        )["art_crop_url"],
        alt
    );
}

#[tokio::test]
async fn commander_detail_query_resolves_a_name_only_legacy_deck() {
    let f = setup().await;
    let detail = f.commander("krenko, mob boss", json!({})).await.unwrap();
    assert_eq!(
        detail["commander"],
        json!({"id": "Krenko, Mob Boss", "name": "Krenko, Mob Boss", "game_changer": false, "image_url": null,
               "art_crop_url": null, "color_identity": null})
    );
    assert_eq!(
        detail["record"],
        json!({"games": 6, "wins": 1, "losses": 4, "draws": 1, "win_rate": 16.7})
    );
}

#[tokio::test]
async fn commander_identity_is_canonical_across_stored_ids_names_seat_order_and_the_overview() {
    let f = setup().await;
    let by_name = f
        .app
        .deck_with(json!({
            "player_id": f.id("Bob"), "name": "Name-only Kangee", "commander_name": "kangee, sky warden",
            "partner_name": "Tymna the Weaver", "color_identity": "WUB",
        }))
        .await;
    let old_printing = f
        .app
        .deck_with(json!({
            "player_id": f.id("Cara"), "name": "Old Kangee", "commander_card_id": "kangee-old-printing",
            "commander_name": "Kangee, Sky Warden", "color_identity": "WU",
        }))
        .await;
    let mut mirror = f.decks.clone();
    mirror.insert("Bob", by_name);
    mirror.insert("Cara", old_printing);
    f.game(
        &mirror,
        "2026-05-01T12:00:00Z",
        "Cara",
        &["Bob", "Alice", "Cara"],
    )
    .await;

    let commanders = f.commanders(json!({})).await;
    let kangee_rows: Vec<&Value> = commanders
        .iter()
        .filter(|row| row["name"] == "Kangee, Sky Warden")
        .collect();
    assert_eq!(kangee_rows.len(), 1);
    assert_eq!(
        pairs(
            &json!(kangee_rows),
            &["id", "games", "wins", "decks", "pilots"]
        )[0],
        [json!("kangee"), json!(9), json!(4), json!(3), json!(3)]
    );
    for row in &commanders {
        let detail = f
            .commander(row["id"].as_str().unwrap(), json!({}))
            .await
            .unwrap_or_else(|| panic!("{row} has no detail"));
        assert_eq!(detail["commander"]["id"], row["id"]);
        assert_eq!(detail["record"]["games"], row["games"]);
    }
    assert_eq!(
        f.commander("kangee-old-printing", json!({})).await.unwrap()["commander"]["id"],
        "kangee"
    );
    assert_eq!(
        f.commander("Kangee, Sky Warden", json!({})).await.unwrap()["record"]["games"],
        9
    );

    let detail = f.commander("kangee", json!({})).await.unwrap();
    assert_eq!(
        detail["record"],
        json!({"games": 9, "wins": 4, "losses": 4, "draws": 1, "win_rate": 44.4})
    );
    assert_eq!(
        detail["win_rate_over_time"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["win_rate"],
        44.4
    );
    assert_eq!(detail["recent_games"][0]["result"], "win");
    assert!(
        !detail["partners"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "Kangee, Sky Warden")
    );
    assert!(
        detail["partners"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "Tymna the Weaver" && row["games"] == 1)
    );

    f.game(
        &mirror,
        "2026-05-02T12:00:00Z",
        "Alice",
        &["Cara", "Bob", "Alice"],
    )
    .await;
    f.game(
        &mirror,
        "2026-05-03T12:00:00Z",
        "Bob",
        &["Alice", "Cara", "Bob"],
    )
    .await;
    let reordered = f
        .commander("kangee", json!({"date_from": "2026-05-01"}))
        .await
        .unwrap();
    assert_eq!(
        reordered["record"],
        json!({"games": 9, "wins": 3, "losses": 6, "draws": 0, "win_rate": 33.3})
    );
    assert_eq!(
        reordered["win_rate_over_time"]
            .as_array()
            .unwrap()
            .iter()
            .map(|point| point["win_rate"].clone())
            .collect::<Vec<_>>(),
        [json!(33.3), json!(33.3), json!(33.3)]
    );

    let overview = f.overview(json!({})).await;
    let top: Vec<Value> = f.commanders(json!({})).await.into_iter().take(8).collect();
    assert_eq!(overview["commanders"], Value::Array(top));
    assert!(
        overview["commanders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "Tymna the Weaver")
    );
    assert_eq!(
        find(&overview["commanders"], "id", json!("kangee"))["games"],
        15
    );
}

// EloTest

fn person(id: i64, name: &str) -> Player {
    Player {
        id,
        name: name.into(),
        ..Player::default()
    }
}

fn result(value: &str) -> GameResult {
    GameResult::parse(value).unwrap()
}

/// Games given oldest first, returned newest first like the stats views load them.
fn games(rows: &[(&str, Vec<(&Player, &str)>)]) -> Vec<Game> {
    let mut games: Vec<Game> = rows
        .iter()
        .enumerate()
        .map(|(index, (date, results))| Game {
            id: i64::try_from(index).unwrap() + 1,
            played_at: utc(&format!("{date}T20:00:00Z")),
            seats: results
                .iter()
                .map(|(player, outcome)| Seat {
                    player_id: player.id,
                    player: (*player).clone(),
                    result: result(outcome),
                    ..Seat::default()
                })
                .collect(),
            ..Game::default()
        })
        .collect();
    games.reverse();
    games
}

fn summary(ratings: &[elo::Rating]) -> Vec<(String, i64, i64, usize)> {
    ratings
        .iter()
        .map(|rating| {
            (
                rating.name.clone(),
                rating.rating,
                rating.peak,
                rating.games,
            )
        })
        .collect()
}

#[test]
fn a_win_at_equal_ratings_moves_the_table_by_k_in_total_and_sums_to_zero() {
    let (alice, bob, cara) = (person(1, "Alice"), person(2, "Bob"), person(3, "Cara"));
    let ratings = elo::ratings(
        &games(&[(
            "2026-01-01",
            vec![(&alice, "win"), (&bob, "loss"), (&cara, "loss")],
        )]),
        None,
    );
    assert_eq!(
        summary(&ratings),
        [
            ("Alice".into(), 1016, 1016, 1),
            ("Bob".into(), 992, 1000, 1),
            ("Cara".into(), 992, 1000, 1)
        ]
    );
    assert_eq!(
        ratings
            .iter()
            .map(|rating| rating.history.clone())
            .collect::<Vec<_>>(),
        vec![
            vec![("2026-01-01".to_owned(), 1016)],
            vec![("2026-01-01".to_owned(), 992)],
            vec![("2026-01-01".to_owned(), 992)],
        ]
    );
}

#[test]
fn an_underdog_gains_more_than_the_favourite_would_have_and_losers_are_not_compared() {
    let (alice, bob, cara) = (person(1, "Alice"), person(2, "Bob"), person(3, "Cara"));
    let ratings = elo::ratings(
        &games(&[
            (
                "2026-01-01",
                vec![(&alice, "win"), (&bob, "loss"), (&cara, "loss")],
            ),
            (
                "2026-01-15",
                vec![(&bob, "win"), (&alice, "loss"), (&cara, "loss")],
            ),
        ]),
        None,
    );
    assert_eq!(
        ratings
            .iter()
            .map(|rating| (rating.name.as_str(), rating.rating, rating.peak))
            .collect::<Vec<_>>(),
        [
            ("Bob", 1009, 1009),
            ("Alice", 1007, 1016),
            ("Cara", 984, 1000)
        ]
    );
    assert_eq!(
        ratings
            .iter()
            .find(|rating| rating.name == "Alice")
            .unwrap()
            .history,
        [
            ("2026-01-01".to_owned(), 1016),
            ("2026-01-15".to_owned(), 1007)
        ]
    );
}

#[test]
fn a_drawn_game_between_equal_players_changes_nothing_an_unequal_draw_favours_the_underdog() {
    let (alice, bob) = (person(1, "Alice"), person(2, "Bob"));
    let equal = elo::ratings(
        &games(&[("2026-01-01", vec![(&alice, "draw"), (&bob, "draw")])]),
        None,
    );
    assert_eq!(
        equal.iter().map(|rating| rating.rating).collect::<Vec<_>>(),
        [1000, 1000]
    );
    let unequal = elo::ratings(
        &games(&[
            ("2026-01-01", vec![(&alice, "win"), (&bob, "loss")]),
            ("2026-01-02", vec![(&alice, "draw"), (&bob, "draw")]),
        ]),
        None,
    );
    assert_eq!(
        unequal
            .iter()
            .map(|rating| (rating.name.as_str(), rating.rating))
            .collect::<Vec<_>>(),
        [("Alice", 1015), ("Bob", 985)]
    );
}

#[test]
fn ratings_replay_oldest_first_regardless_of_the_list_order_given() {
    let (alice, bob) = (person(1, "Alice"), person(2, "Bob"));
    let ratings = elo::ratings(
        &games(&[
            ("2026-01-01", vec![(&alice, "win"), (&bob, "loss")]),
            ("2026-01-02", vec![(&bob, "win"), (&alice, "loss")]),
        ]),
        None,
    );
    assert_eq!(
        ratings
            .iter()
            .map(|rating| (rating.name.as_str(), rating.rating))
            .collect::<Vec<_>>(),
        [("Bob", 1001), ("Alice", 999)]
    );
}

#[test]
fn a_window_replays_earlier_games_and_reports_only_the_change_inside_it() {
    let (alice, bob, cara) = (person(1, "Alice"), person(2, "Bob"), person(3, "Cara"));
    let rows = [
        (
            "2026-01-01",
            vec![(&alice, "win"), (&bob, "loss"), (&cara, "loss")],
        ),
        ("2026-02-01", vec![(&bob, "win"), (&alice, "loss")]),
    ];
    let window = (
        the_gathering::local_time::parse_date("2026-01-20").unwrap(),
        utc("2026-01-20T05:00:00Z"),
    );
    let ratings = elo::ratings(&games(&rows), Some(window));
    let full = elo::ratings(&games(&rows), None);
    assert_eq!(
        ratings
            .iter()
            .map(|rating| (rating.name.clone(), rating.rating))
            .collect::<Vec<_>>(),
        full.iter()
            .filter(|rating| rating.name != "Cara")
            .map(|rating| (rating.name.clone(), rating.rating))
            .collect::<Vec<_>>()
    );
    let alice_rating = ratings
        .iter()
        .find(|rating| rating.name == "Alice")
        .unwrap();
    let final_rating = full
        .iter()
        .find(|rating| rating.name == "Alice")
        .unwrap()
        .rating;
    assert_eq!(
        (alice_rating.start, alice_rating.peak, alice_rating.games),
        (1016, 1016, 1)
    );
    assert_eq!(
        alice_rating.history,
        [
            ("2026-01-20".to_owned(), 1016),
            ("2026-02-01".to_owned(), final_rating)
        ]
    );
}

// RecordsTest

#[test]
fn histogram_bins_from_the_lowest_to_the_highest_value_with_empty_bins_between() {
    assert_eq!(
        Value::Array(records::histogram(
            [Some(62), Some(75), Some(14), None, Some(121)],
            15
        )),
        json!([
            {"from": 0, "to": 15, "games": 1},
            {"from": 15, "to": 30, "games": 0},
            {"from": 30, "to": 45, "games": 0},
            {"from": 45, "to": 60, "games": 0},
            {"from": 60, "to": 75, "games": 1},
            {"from": 75, "to": 90, "games": 1},
            {"from": 90, "to": 105, "games": 0},
            {"from": 105, "to": 120, "games": 0},
            {"from": 120, "to": 135, "games": 1},
        ])
    );
}

#[test]
fn histogram_treats_the_upper_edge_as_exclusive_and_skips_missing_values_entirely() {
    assert_eq!(
        Value::Array(records::histogram([Some(9), Some(10), Some(11)], 2)),
        json!([{"from": 8, "to": 10, "games": 1}, {"from": 10, "to": 12, "games": 2}])
    );
    assert!(records::histogram([None, None], 2).is_empty());
}

#[test]
fn matchups_record_each_players_results_only_in_games_shared_with_the_opponent() {
    let (alice, bob, cara) = (person(1, "Alice"), person(2, "Bob"), person(3, "Cara"));
    let rows = games(&[
        (
            "2026-01-01",
            vec![(&alice, "win"), (&bob, "loss"), (&cara, "loss")],
        ),
        ("2026-01-02", vec![(&bob, "win"), (&alice, "loss")]),
        ("2026-01-03", vec![(&cara, "win"), (&bob, "loss")]),
    ]);
    let matchups = Value::Array(records::matchups(rows.iter()));
    let pair = |id: i64, opponent: i64| {
        matchups
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id && row["opponent_id"] == opponent)
            .unwrap()
            .clone()
    };
    assert_eq!(
        pair(1, 3),
        json!({"id": 1, "name": "Alice", "opponent_id": 3, "games": 1, "wins": 1, "losses": 0, "draws": 0, "win_rate": 100.0})
    );
    assert_eq!(
        pairs(&json!([pair(1, 2)]), &["games", "wins", "win_rate"])[0],
        [json!(2), json!(1), json!(50.0)]
    );
    assert_eq!(
        pairs(&json!([pair(2, 1)]), &["games", "wins", "win_rate"])[0],
        [json!(2), json!(1), json!(50.0)]
    );
    assert_eq!(
        pairs(&json!([pair(2, 3)]), &["games", "wins", "losses"])[0],
        [json!(2), json!(0), json!(2)]
    );
    assert_eq!(matchups.as_array().unwrap().len(), 6);
}

#[test]
fn color_exposure_counts_a_seat_once_per_color_in_its_deck_and_reports_the_share_of_deck_bearing_seats()
 {
    let seat = |outcome: &str, identity: Option<&str>| Seat {
        result: result(outcome),
        deck: identity.map(|identity| Deck {
            color_identity: identity.into(),
            ..Deck::default()
        }),
        ..Seat::default()
    };
    let seats = [
        seat("win", Some("GUW")),
        seat("loss", Some("R")),
        seat("loss", Some("WU")),
        seat("win", None),
    ];
    let rows = Value::Array(records::color_exposure(seats.iter()));
    assert_eq!(
        pairs(&rows, &["id", "games", "wins", "win_rate", "share"]),
        vec![
            vec![json!("W"), json!(2), json!(1), json!(50.0), json!(66.7)],
            vec![json!("U"), json!(2), json!(1), json!(50.0), json!(66.7)],
            vec![json!("B"), json!(0), json!(0), json!(0.0), json!(0.0)],
            vec![json!("R"), json!(1), json!(0), json!(0.0), json!(33.3)],
            vec![json!("G"), json!(1), json!(1), json!(100.0), json!(33.3)],
        ]
    );
    assert_eq!(rows[0]["name"], "White");
}
