//! Players, decks, and games (color identity, deck picker, summary card).
// Test crates: helpers outside `#[test]` functions may unwrap and index freely, like the
// tests themselves (clippy.toml only exempts `#[test]` bodies).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::assert_is_empty
)]

use crate::support;

use serde_json::{Value, json};
use support::{TestApp, utc};
use the_gathering::error::Errors;
use the_gathering::games::deck_picker::{Candidate, selection_weights};
use the_gathering::games::{
    ArtFetcher, Deck, Game, GameResult, GameSource, GamesError, Outcome, Player, Seat,
    color_identity, summary_card,
};

fn invalid<T: std::fmt::Debug>(result: Result<T, GamesError>) -> Errors {
    match result {
        Err(GamesError::Invalid(errors)) => errors,
        other => panic!("expected validation errors, got {other:?}"),
    }
}

fn seats(players: &[&Player], winner: usize) -> Vec<Value> {
    players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            json!({"player_id": player.id, "seat": index + 1, "result": if index == winner { "win" } else { "loss" }})
        })
        .collect()
}

fn game_attrs(players: &[&Player]) -> Value {
    json!({"played_at": "2026-09-19T18:00:00Z", "source": "manual", "seats": seats(players, 0)})
}

fn with(mut base: Value, extra: Value) -> Value {
    if let (Value::Object(base), Value::Object(extra)) = (&mut base, extra) {
        base.extend(extra);
    }
    base
}

async fn players(app: &TestApp, count: usize) -> Vec<Player> {
    let mut players = Vec::new();
    for index in 1..=count {
        players.push(app.player(&format!("Player {index}")).await);
    }
    players
}

#[tokio::test]
async fn formats_retain_their_winner_cardinality_and_imports_default_to_commander() {
    let app = TestApp::new().await;
    let games = &app.state.games;
    let players = players(&app, 4).await;
    let refs: Vec<&Player> = players.iter().collect();
    let attrs = game_attrs(&refs);
    let mut two_winners = attrs.clone();
    two_winners["seats"][1]["result"] = json!("win");

    let errors = invalid(
        games
            .create_game(
                &with(attrs.clone(), json!({"format": "two_headed_giant"})),
                None,
            )
            .await,
    );
    assert!(
        errors
            .messages("seats")
            .contains(&"must have exactly two winners or all draws".to_owned())
    );
    let game = games
        .create_game(
            &with(two_winners.clone(), json!({"format": "two_headed_giant"})),
            None,
        )
        .await
        .unwrap();
    assert_eq!(game.format.as_str(), "two_headed_giant");
    assert_eq!(
        game.seats
            .iter()
            .filter(|seat| seat.result == GameResult::Win)
            .count(),
        2
    );

    for format in ["commander", "five_star"] {
        let errors = invalid(
            games
                .create_game(&with(two_winners.clone(), json!({"format": format})), None)
                .await,
        );
        assert!(
            errors
                .messages("seats")
                .contains(&"must have exactly one winner or all draws".to_owned())
        );
    }

    let game = games.create_game(&attrs, None).await.unwrap();
    assert_eq!(game.format.as_str(), "commander");
    let errors = invalid(
        games
            .create_game(&with(attrs.clone(), json!({"format": "invalid"})), None)
            .await,
    );
    assert_eq!(errors.messages("format"), ["is invalid"]);
    let mut draws = attrs.clone();
    for seat in draws["seats"].as_array_mut().unwrap() {
        seat["result"] = json!("draw");
    }
    games
        .create_game(&with(draws, json!({"format": "two_headed_giant"})), None)
        .await
        .unwrap();
}

#[tokio::test]
async fn enforces_the_two_to_ten_seat_bounds_at_both_edges() {
    let app = TestApp::new().await;
    let players = players(&app, 11).await;
    let refs: Vec<&Player> = players.iter().collect();

    let errors = invalid(
        app.state
            .games
            .create_game(&game_attrs(&refs[..1]), None)
            .await,
    );
    assert!(
        errors
            .messages("seats")
            .contains(&"must contain between 2 and 10 players".to_owned())
    );

    let game = app
        .state
        .games
        .create_game(&game_attrs(&refs[..10]), None)
        .await
        .unwrap();
    assert_eq!(game.seats.len(), 10);

    let errors = invalid(app.state.games.create_game(&game_attrs(&refs), None).await);
    let rows = errors.nested("seats");
    assert_eq!(rows.len(), 11);
    assert_eq!(
        rows[10].messages("seat"),
        ["must be less than or equal to 10"]
    );
    // Like Ecto's traverse_errors, the JSON lists the rows only.
    assert_eq!(
        errors.to_json()["seats"][10],
        json!({"seat": ["must be less than or equal to 10"]})
    );
    assert_eq!(errors.to_json()["seats"][0], json!({}));
}

#[tokio::test]
async fn keeps_unknown_kills_distinct_from_zero_and_validates_kill_counts() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let [first, second]: [Value; 2] = seats(&[&alice, &bob], 0).try_into().unwrap();

    let mut zero = first.clone();
    zero["kills"] = json!(0);
    let game = app
        .state
        .games
        .create_game(
            &with(
                game_attrs(&[&alice, &bob]),
                json!({"seats": [zero, second.clone()]}),
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        game.seats.iter().map(|seat| seat.kills).collect::<Vec<_>>(),
        [Some(0), None]
    );

    for kills in [json!(-1), json!(1.5), json!(10)] {
        let mut seat = first.clone();
        seat["kills"] = kills;
        let attrs = with(
            game_attrs(&[&alice, &bob]),
            json!({"seats": [seat, second.clone()]}),
        );
        let errors = invalid(app.state.games.create_game(&attrs, None).await);
        assert_eq!(errors.nested("seats")[0].messages("kills").len(), 1);
    }
}

#[tokio::test]
async fn rejects_a_duplicate_player_even_when_seat_numbers_differ() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let mut attrs = game_attrs(&[&alice, &bob]);
    attrs["seats"][1]["player_id"] = json!(alice.id);
    let errors = invalid(app.state.games.create_game(&attrs, None).await);
    assert!(
        errors
            .messages("seats")
            .contains(&"cannot contain the same player twice".to_owned())
    );
}

#[tokio::test]
async fn rejects_a_deck_belonging_to_another_player() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let deck = app.deck(alice.id, "Birds", "Kangee").await;
    let mut attrs = game_attrs(&[&alice, &bob]);
    attrs["seats"][1]["deck_id"] = json!(deck.id);
    let errors = invalid(app.state.games.create_game(&attrs, None).await);
    assert!(
        errors
            .messages("seats")
            .contains(&"contains a deck that does not belong to its player".to_owned())
    );
}

#[tokio::test]
async fn rejects_two_winners_and_accepts_an_all_draw_game() {
    let app = TestApp::new().await;
    let players = [
        app.player("Alice").await,
        app.player("Bob").await,
        app.player("Cara").await,
    ];
    let refs: Vec<&Player> = players.iter().collect();
    let mut attrs = game_attrs(&refs);
    attrs["seats"][1]["result"] = json!("win");
    let errors = invalid(app.state.games.create_game(&attrs, None).await);
    assert!(
        errors
            .messages("seats")
            .contains(&"must have exactly one winner or all draws".to_owned())
    );

    for seat in attrs["seats"].as_array_mut().unwrap() {
        seat["result"] = json!("draw");
    }
    let game = app.state.games.create_game(&attrs, None).await.unwrap();
    assert!(
        game.seats
            .iter()
            .all(|seat| seat.result == GameResult::Draw)
    );
}

#[tokio::test]
async fn external_ids_are_idempotent_within_a_source_but_independent_across_sources() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let attrs = with(
        game_attrs(&[&alice, &bob]),
        json!({"source": "csv", "external_id": "row-42"}),
    );
    let first = app.state.games.create_game(&attrs, None).await.unwrap();
    let repeated = app
        .state
        .games
        .create_game(
            &with(attrs.clone(), json!({"notes": "ignored on replay"})),
            None,
        )
        .await
        .unwrap();
    assert_eq!(first.id, repeated.id);
    assert_eq!(repeated.notes, None);
    let discord = app
        .state
        .games
        .create_game(&with(attrs, json!({"source": "discord"})), None)
        .await
        .unwrap();
    assert_ne!(discord.id, first.id);
    assert_eq!(discord.source, GameSource::Discord);
}

#[tokio::test]
async fn upserting_by_external_id_updates_the_existing_game() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let games = &app.state.games;
    let created = games
        .upsert_game_by_external_id("discord", "spellbot:SB1", &game_attrs(&[&alice, &bob]))
        .await
        .unwrap();
    let updated = games
        .upsert_game_by_external_id(
            "discord",
            "spellbot:SB1",
            &with(game_attrs(&[&alice, &bob]), json!({"notes": "edited"})),
        )
        .await
        .unwrap();
    assert_eq!(created.id, updated.id);
    assert_eq!(updated.notes.as_deref(), Some("edited"));
    let found = games
        .find_or_create_game_by_external_id("discord", "spellbot:SB1", &json!({}))
        .await
        .unwrap();
    assert_eq!(found.id, created.id);
}

#[tokio::test]
async fn player_names_are_unique_case_insensitively_and_finder_returns_the_existing_player() {
    let app = TestApp::new().await;
    let games = &app.state.games;
    let alice = games
        .create_player(&json!({"name": "Alice"}), None)
        .await
        .unwrap();
    let errors = invalid(
        games
            .create_player(&json!({"name": "  ALICE  "}), None)
            .await,
    );
    assert!(
        errors
            .messages("name")
            .contains(&"has already been taken".to_owned())
    );
    let found = games
        .find_or_create_player_by_name("alice", &json!({}))
        .await
        .unwrap();
    assert_eq!(found.id, alice.id);
}

#[tokio::test]
async fn player_resolver_surfaces_failures_while_linking_an_existing_discord_identity() {
    let app = TestApp::new().await;
    let player = app
        .player_with(
            json!({"name": "Discord Player", "discord_id": "discord-42"}),
            None,
        )
        .await;
    match app
        .state
        .games
        .resolve_player("Renamed Player", Some("discord-42"), Some(-1))
        .await
    {
        Err(the_gathering::games::ResolveError::Invalid(errors)) => {
            assert_eq!(errors.messages("user_id"), ["does not exist"]);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(
        app.state
            .games
            .get_player(player.id)
            .await
            .unwrap()
            .unwrap()
            .user_id,
        None
    );
}

#[tokio::test]
async fn name_finders_fold_case_like_sqlite_so_non_ascii_names_are_found_instead_of_reinserted() {
    let app = TestApp::new().await;
    let games = &app.state.games;
    let eowyn = games
        .create_player(&json!({"name": "Éowyn"}), None)
        .await
        .unwrap();
    let found = games
        .find_or_create_player_by_name("Éowyn", &json!({}))
        .await
        .unwrap();
    assert_eq!(found.id, eowyn.id);

    let attrs = json!({"commander_name": "Éowyn, Shieldmaiden"});
    let deck = games
        .find_or_create_deck(eowyn.id, "Éowyn, Shieldmaiden", &attrs)
        .await
        .unwrap();
    let same = games
        .find_or_create_deck(eowyn.id, "Éowyn, Shieldmaiden", &attrs)
        .await
        .unwrap();
    assert_eq!(same.id, deck.id);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM decks")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);

    let errors = invalid(
        games.create_deck(&json!({"player_id": eowyn.id, "name": "Éowyn, Shieldmaiden", "commander_name": "x"})).await,
    );
    assert!(
        errors
            .messages("name")
            .contains(&"has already been taken".to_owned())
    );
}

#[tokio::test]
async fn deck_identities_always_include_both_commanders_colors_and_keep_chosen_extras() {
    let app = TestApp::new().await;
    app.card("doctor", "The Tenth Doctor", &["R", "U"], json!({}), true)
        .await;
    app.card("clara", "Clara Oswald", &[], json!({}), true)
        .await;
    app.card("tymna", "Tymna the Weaver", &["W", "B"], json!({}), true)
        .await;
    let doctor = app.player("Doctor Who").await;
    let games = &app.state.games;

    let deck = games
        .create_deck(&json!({
            "player_id": doctor.id,
            "name": "Allons-y",
            "commander_card_id": "doctor",
            "commander_name": "The Tenth Doctor",
            "partner_name": "Tymna the Weaver",
            "color_identity": "UR",
        }))
        .await
        .unwrap();
    assert_eq!(deck.color_identity, "WUBR");

    let deck = games
        .update_deck(&deck, &json!({"partner_card_id": "clara", "partner_name": "Clara Oswald", "color_identity": "G"}))
        .await
        .unwrap();
    assert_eq!(deck.color_identity, "URG");

    let errors = invalid(
        games
            .update_deck(&deck, &json!({"color_identity": "UURG"}))
            .await,
    );
    assert!(
        errors
            .messages("color_identity")
            .contains(&"must contain each of W, U, B, R, and G at most once".to_owned())
    );
}

#[tokio::test]
async fn deck_printings_must_belong_to_the_selected_card() {
    let app = TestApp::new().await;
    app.card("kangee", "Kangee, Sky Warden", &["W", "U"], json!({}), true)
        .await;
    app.card("krenko", "Krenko, Mob Boss", &["R"], json!({}), true)
        .await;
    app.printing("kangee-alt", "kangee", "Kangee, Sky Warden", json!({}))
        .await;
    let alice = app.player("Alice").await;
    let games = &app.state.games;
    let deck = games
        .create_deck(&json!({
            "player_id": alice.id, "name": "Birds", "commander_card_id": "kangee",
            "commander_name": "Kangee, Sky Warden", "commander_printing_id": "kangee-alt",
        }))
        .await
        .unwrap();
    assert_eq!(deck.commander_printing_id.as_deref(), Some("kangee-alt"));

    let errors = invalid(
        games
            .update_deck(&deck, &json!({"commander_card_id": "krenko", "commander_name": "Krenko, Mob Boss", "commander_printing_id": "kangee-alt"}))
            .await,
    );
    assert_eq!(
        errors.messages("commander_printing_id"),
        ["must be a printing of the selected card"]
    );

    // Changing the commander without naming a printing clears the old one.
    let deck = games
        .update_deck(
            &deck,
            &json!({"commander_card_id": "krenko", "commander_name": "Krenko, Mob Boss"}),
        )
        .await
        .unwrap();
    assert_eq!(deck.commander_printing_id, None);
    assert_eq!(deck.color_identity, "WUR");
}

#[tokio::test]
async fn players_carry_the_linked_users_avatar_and_nil_when_unlinked_or_the_user_has_none() {
    let app = TestApp::new().await;
    let linked_user = app.unique_member().await;
    sqlx::query("UPDATE users SET avatar_url = 'https://cdn/av.png' WHERE id = ?")
        .bind(linked_user.id)
        .execute(app.pool())
        .await
        .unwrap();
    let bare_user = app.unique_member().await;
    let linked = app
        .player_with(json!({"name": "Linked"}), Some(linked_user.id))
        .await;
    let bare = app
        .player_with(json!({"name": "Bare"}), Some(bare_user.id))
        .await;
    let unlinked = app.player("Unlinked").await;

    let avatars: std::collections::HashMap<i64, Option<String>> = app
        .state
        .games
        .list_players(false)
        .await
        .unwrap()
        .into_iter()
        .map(|player| (player.id, player.avatar_url))
        .collect();
    assert_eq!(avatars.len(), 3);
    assert_eq!(avatars[&linked.id].as_deref(), Some("https://cdn/av.png"));
    assert_eq!(avatars[&bare.id], None);
    assert_eq!(avatars[&unlinked.id], None);
    let detail = app
        .state
        .games
        .get_player_detail(linked.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        detail.player.avatar_url.as_deref(),
        Some("https://cdn/av.png")
    );
}

#[tokio::test]
async fn invalid_user_references_return_changeset_errors() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let errors = invalid(
        app.state
            .games
            .create_player(&json!({"name": "Orphan"}), Some(999_999))
            .await,
    );
    assert_eq!(errors.messages("user_id"), ["does not exist"]);
    let errors = invalid(
        app.state
            .games
            .create_game(&game_attrs(&[&alice, &bob]), Some(999_999))
            .await,
    );
    assert_eq!(errors.messages("created_by_user_id"), ["does not exist"]);
}

#[tokio::test]
async fn find_deck_falls_back_from_name_to_an_order_insensitive_commander_pairing() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let party = app
        .deck(alice.id, "Party time", "Gandalf, Party Guest")
        .await;
    let partners = app
        .deck_with(json!({
            "player_id": alice.id, "name": "Tyvar + Ellivere",
            "commander_name": "Tyvar, the Bellicose", "partner_name": "Ellivere of the Wild Court",
        }))
        .await;
    app.deck(alice.id, "Solo Tyvar", "Tyvar, the Bellicose")
        .await;
    let games = &app.state.games;
    let find = |player: i64,
                name: &'static str,
                commander: Option<&'static str>,
                partner: Option<&'static str>| async move {
        games
            .find_deck(player, name, commander, partner)
            .await
            .unwrap()
            .map(|deck: Deck| deck.id)
    };

    assert_eq!(
        find(alice.id, "party TIME", Some("Something else"), None).await,
        Some(party.id)
    );
    assert_eq!(
        find(
            alice.id,
            "Gandalf, Party Guest",
            Some("gandalf, party guest"),
            None
        )
        .await,
        Some(party.id)
    );
    let reused = games
        .find_or_create_deck(
            alice.id,
            "Gandalf",
            &json!({"commander_name": "Gandalf, Party Guest"}),
        )
        .await
        .unwrap();
    assert_eq!(reused.id, party.id);
    assert_eq!(
        find(
            alice.id,
            "x",
            Some("Ellivere of the Wild Court"),
            Some("Tyvar, the Bellicose")
        )
        .await,
        Some(partners.id)
    );
    assert_eq!(
        find(
            alice.id,
            "x",
            Some("Tyvar, the Bellicose"),
            Some("Ellivere of the Wild Court")
        )
        .await,
        Some(partners.id)
    );
    assert_eq!(
        find(
            alice.id,
            "x",
            Some("Tyvar, the Bellicose"),
            Some("Someone Else")
        )
        .await,
        None
    );
    assert_eq!(
        find(bob.id, "x", Some("Gandalf, Party Guest"), None).await,
        None
    );
    assert_eq!(find(alice.id, "x", None, None).await, None);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM decks")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(count, 3);
}

async fn seat_deck(app: &TestApp, game_id: i64, player_id: i64) -> Option<i64> {
    sqlx::query_scalar("SELECT deck_id FROM game_players WHERE game_id = ? AND player_id = ?")
        .bind(game_id)
        .bind(player_id)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn deleting_a_deck_moves_its_seats_to_a_replacement_of_the_same_player_or_clears_them() {
    let app = TestApp::new().await;
    let games = &app.state.games;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let dupe = games
        .find_or_create_deck(alice.id, "Dupe", &json!({"commander_name": "Krenko"}))
        .await
        .unwrap();
    let keeper = games
        .find_or_create_deck(
            alice.id,
            "Keeper",
            &json!({"commander_name": "Krenko, Mob Boss"}),
        )
        .await
        .unwrap();
    let bobs = games
        .find_or_create_deck(bob.id, "Bob's", &json!({"commander_name": "Krenko"}))
        .await
        .unwrap();
    let game = app
        .game(
            json!({"played_at": "2026-09-19T18:00:00Z", "seats": [
                {"player_id": alice.id, "deck_id": dupe.id, "seat": 1, "result": "win"},
                {"player_id": bob.id, "deck_id": bobs.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;

    assert!(matches!(
        games.delete_deck(&dupe, Some(&bobs)).await,
        Err(GamesError::BadRequest)
    ));
    assert!(matches!(
        games.delete_deck(&dupe, Some(&dupe)).await,
        Err(GamesError::BadRequest)
    ));
    assert_eq!(seat_deck(&app, game.id, alice.id).await, Some(dupe.id));

    let deleted = games.delete_deck(&dupe, Some(&keeper)).await.unwrap();
    assert_eq!(deleted.id, dupe.id);
    assert!(games.get_deck(dupe.id).await.unwrap().is_none());
    assert_eq!(seat_deck(&app, game.id, alice.id).await, Some(keeper.id));

    games.delete_deck(&keeper, None).await.unwrap();
    assert_eq!(seat_deck(&app, game.id, alice.id).await, None);
    assert_eq!(seat_deck(&app, game.id, bob.id).await, Some(bobs.id));
}

#[tokio::test]
async fn merging_players_moves_seats_and_decks_collapses_same_named_decks_and_carries_identity() {
    let app = TestApp::new().await;
    let games = &app.state.games;
    let drew = app.player("Drew").await;
    let alice = app.player("Alice").await;
    let wax = app
        .player_with(
            json!({"name": "waxpoetik", "discord_id": "123456789"}),
            None,
        )
        .await;
    let drew_krenko = games
        .find_or_create_deck(drew.id, "Krenko", &json!({"commander_name": "Krenko"}))
        .await
        .unwrap();
    let wax_krenko = games
        .find_or_create_deck(wax.id, "krenko", &json!({"commander_name": "Krenko"}))
        .await
        .unwrap();
    let wax_tifa = games
        .find_or_create_deck(wax.id, "Tifa", &json!({"commander_name": "Tifa"}))
        .await
        .unwrap();
    let game_a = app
        .game(
            json!({"played_at": "2026-09-19T18:00:00Z", "seats": [
                {"player_id": drew.id, "deck_id": drew_krenko.id, "seat": 1, "result": "win"},
                {"player_id": alice.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;
    let game_b = app
        .game(
            json!({"played_at": "2026-09-19T18:00:00Z", "external_id": "b", "seats": [
                {"player_id": wax.id, "deck_id": wax_krenko.id, "seat": 1, "result": "loss"},
                {"player_id": alice.id, "seat": 2, "result": "win"},
            ]}),
            None,
        )
        .await;

    let merged = games.merge_players(&wax, &drew).await.unwrap();
    assert_eq!(merged.id, drew.id);
    assert_eq!(merged.discord_id.as_deref(), Some("123456789"));
    assert!(games.get_player(wax.id).await.unwrap().is_none());

    let game_b = games.get_game(game_b.id).await.unwrap().unwrap();
    let seat_one: Vec<&Seat> = game_b.seats.iter().filter(|seat| seat.seat == 1).collect();
    assert_eq!(seat_one.len(), 1);
    assert_eq!(
        (seat_one[0].player_id, seat_one[0].deck_id),
        (drew.id, Some(drew_krenko.id))
    );
    assert!(games.get_deck(wax_krenko.id).await.unwrap().is_none());
    assert_eq!(
        games
            .get_deck(wax_tifa.id)
            .await
            .unwrap()
            .unwrap()
            .player_id,
        drew.id
    );
    assert_eq!(
        games
            .get_player_detail(drew.id)
            .await
            .unwrap()
            .unwrap()
            .seats
            .len(),
        2
    );
    let mut ids: Vec<i64> = games
        .get_game(game_a.id)
        .await
        .unwrap()
        .unwrap()
        .seats
        .iter()
        .map(|seat| seat.player_id)
        .collect();
    ids.sort_unstable();
    let mut expected = vec![drew.id, alice.id];
    expected.sort_unstable();
    assert_eq!(ids, expected);

    let errors = invalid(games.merge_players(&alice, &merged).await);
    assert_eq!(
        errors.messages("merge"),
        ["both players are seated in the same game"]
    );
    assert!(matches!(
        games.merge_players(&merged, &merged).await,
        Err(GamesError::BadRequest)
    ));
}

#[tokio::test]
async fn merging_players_migrates_eliminated_by_references() {
    let app = TestApp::new().await;
    let source = app.player("Source").await;
    let target = app.player("Target").await;
    let defeated = app.player("Defeated").await;
    let winner = app.player("Winner").await;
    let game = app
        .simple_game("2026-09-19T18:00:00Z", winner.id, defeated.id, None)
        .await;
    let seat = game
        .seats
        .iter()
        .find(|seat| seat.player_id == defeated.id)
        .unwrap();
    sqlx::query(
        "UPDATE game_players SET eliminated_by_player_id = ?, eliminated_turn = 8 WHERE id = ?",
    )
    .bind(source.id)
    .bind(seat.id)
    .execute(app.pool())
    .await
    .unwrap();
    let merged = app
        .state
        .games
        .merge_players(&source, &target)
        .await
        .unwrap();
    assert_eq!(merged.id, target.id);
    let by: Option<i64> =
        sqlx::query_scalar("SELECT eliminated_by_player_id FROM game_players WHERE id = ?")
            .bind(seat.id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(by, Some(target.id));
}

#[tokio::test]
async fn merging_refuses_a_player_seated_at_an_open_webcam_table() {
    let app = TestApp::new().await;
    let source = app.player("Seated").await;
    let target = app.player("Target").await;
    app.state
        .games
        .set_seated_check(std::sync::Arc::new(|_| Box::pin(async { true })));
    let errors = invalid(app.state.games.merge_players(&source, &target).await);
    assert!(errors.messages("merge")[0].starts_with("Seated has a seat at an open webcam table"));
}

#[tokio::test]
async fn linking_a_player_to_an_account_merges_the_accounts_stub_player_into_it() {
    let app = TestApp::new().await;
    let games = &app.state.games;
    let user = app.unique_member().await;
    let imported = app.player("Drew").await;
    let stub = app
        .player_with(
            json!({"name": "Drew (2)", "discord_id": "42"}),
            Some(user.id),
        )
        .await;
    let other = app.player("Other").await;
    app.simple_game("2026-09-19T18:00:00Z", stub.id, other.id, None)
        .await;

    let linked = games.link_player_to_user(&imported, &user).await.unwrap();
    assert_eq!(linked.id, imported.id);
    assert_eq!(linked.user_id, Some(user.id));
    assert_eq!(linked.discord_id.as_deref(), Some("42"));
    assert!(games.get_player(stub.id).await.unwrap().is_none());
    assert_eq!(
        games
            .get_player_detail(imported.id)
            .await
            .unwrap()
            .unwrap()
            .seats
            .len(),
        1
    );

    let again = games.link_player_to_user(&imported, &user).await.unwrap();
    assert_eq!(again.id, imported.id);
    let other_user = app.unique_member().await;
    let errors = invalid(games.link_player_to_user(&imported, &other_user).await);
    assert_eq!(
        errors.messages("merge"),
        ["players belong to different accounts"]
    );
}

async fn ids(app: &TestApp, opts: Value) -> Vec<i64> {
    app.state
        .games
        .list_games(&opts)
        .await
        .unwrap()
        .0
        .iter()
        .map(|game| game.id)
        .collect()
}

#[tokio::test]
async fn list_games_combines_filters_paginates_and_orders_newest_first() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let cara = app.player("Cara").await;
    let birds = app.deck(alice.id, "Birds", "Kangee").await;
    let old = app
        .game(
            json!({"played_at": "2026-09-01T12:00:00Z", "seats": [
                {"player_id": alice.id, "deck_id": birds.id, "seat": 1, "result": "win"},
                {"player_id": bob.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;
    let middle = app
        .simple_game("2026-09-10T12:00:00Z", alice.id, cara.id, None)
        .await;
    let newest = app
        .simple_game("2026-09-18T12:00:00Z", bob.id, cara.id, None)
        .await;

    let (page_one, pagination) = app
        .state
        .games
        .list_games(&json!({"page": 1, "per_page": 2}))
        .await
        .unwrap();
    assert_eq!(
        page_one.iter().map(|game| game.id).collect::<Vec<_>>(),
        [newest.id, middle.id]
    );
    assert_eq!(
        serde_json::to_value(pagination).unwrap(),
        json!({"page": 1, "per_page": 2, "total": 3, "total_pages": 2})
    );
    assert_eq!(ids(&app, json!({"page": 2, "per_page": 2})).await, [old.id]);
    assert_eq!(
        ids(
            &app,
            json!({"player_id": alice.id, "date_from": "2026-09-05", "date_to": "2026-09-15"})
        )
        .await,
        [middle.id]
    );
    assert_eq!(ids(&app, json!({"deck_id": birds.id})).await, [old.id]);
}

#[tokio::test]
async fn list_games_filters_by_winner_commander_seat_count_turns_and_duration() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let cara = app.player("Cara").await;
    let birds = app.deck(alice.id, "Birds", "Kangee, Sky Warden").await;
    let partners = app
        .deck_with(json!({"player_id": bob.id, "name": "Partners", "commander_name": "Thrasios", "partner_name": "Tymna the Weaver"}))
        .await;
    let alice_win = app
        .game(
            json!({"played_at": "2026-09-19T18:00:00Z", "turns": 12, "duration_minutes": 95, "seats": [
                {"player_id": alice.id, "deck_id": birds.id, "seat": 1, "result": "win"},
                {"player_id": bob.id, "deck_id": partners.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;
    let bob_win = app
        .game(
            json!({"played_at": "2026-09-19T18:00:00Z", "turns": 6, "duration_minutes": 40, "seats": [
                {"player_id": bob.id, "seat": 1, "result": "win"},
                {"player_id": alice.id, "seat": 2, "result": "loss"},
                {"player_id": cara.id, "seat": 3, "result": "loss"},
            ]}),
            None,
        )
        .await;

    assert_eq!(
        ids(&app, json!({"winner_id": bob.id.to_string()})).await,
        [bob_win.id]
    );
    assert_eq!(
        ids(&app, json!({"winner_id": alice.id})).await,
        [alice_win.id]
    );
    assert_eq!(
        ids(&app, json!({"commander": "kangee"})).await,
        [alice_win.id]
    );
    assert_eq!(
        ids(&app, json!({"commander": "TYMNA"})).await,
        [alice_win.id]
    );
    assert_eq!(
        ids(&app, json!({"commander": "   "})).await,
        [bob_win.id, alice_win.id]
    );
    assert!(ids(&app, json!({"commander": "Atraxa"})).await.is_empty());
    assert_eq!(ids(&app, json!({"player_count": 3})).await, [bob_win.id]);
    assert_eq!(
        ids(&app, json!({"player_count": "2"})).await,
        [alice_win.id]
    );
    assert_eq!(ids(&app, json!({"min_turns": 7})).await, [alice_win.id]);
    assert_eq!(
        ids(&app, json!({"max_turns": 12})).await,
        [bob_win.id, alice_win.id]
    );
    assert_eq!(ids(&app, json!({"max_turns": 11})).await, [bob_win.id]);
    assert_eq!(
        ids(&app, json!({"min_duration": 40, "max_duration": 60})).await,
        [bob_win.id]
    );
    assert_eq!(
        ids(&app, json!({"min_duration": "junk"})).await,
        [bob_win.id, alice_win.id]
    );
}

#[tokio::test]
async fn list_games_filters_by_colors_win_condition_seat_opponent_and_player_result() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let cara = app.player("Cara").await;
    let golgari = app.deck_with(json!({"player_id": alice.id, "name": "Rot", "commander_name": "Meren", "color_identity": "GB"})).await;
    let azorius = app.deck_with(json!({"player_id": bob.id, "name": "Walls", "commander_name": "Hanna", "color_identity": "WU"})).await;
    let colorless =
        app.deck_with(json!({"player_id": cara.id, "name": "Artifacts", "commander_name": "Karn", "color_identity": ""})).await;
    let golgari_win = app
        .game(
            json!({"played_at": "2026-09-19T18:00:00Z", "win_condition": "infinite_combo", "seats": [
                {"player_id": alice.id, "deck_id": golgari.id, "seat": 1, "result": "win"},
                {"player_id": bob.id, "deck_id": azorius.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;
    let azorius_win = app
        .game(
            json!({"played_at": "2026-09-19T18:00:00Z", "win_condition": "damage", "seats": [
                {"player_id": alice.id, "deck_id": golgari.id, "seat": 1, "result": "loss"},
                {"player_id": bob.id, "deck_id": azorius.id, "seat": 2, "result": "win"},
                {"player_id": cara.id, "deck_id": colorless.id, "seat": 3, "result": "loss"},
            ]}),
            None,
        )
        .await;
    let both = vec![azorius_win.id, golgari_win.id];

    assert_eq!(ids(&app, json!({"colors": "BG"})).await, both);
    assert_eq!(
        ids(&app, json!({"winner_colors": "BG"})).await,
        [golgari_win.id]
    );
    assert_eq!(
        ids(&app, json!({"winner_colors": "UW"})).await,
        [azorius_win.id]
    );
    assert!(ids(&app, json!({"colors": "B"})).await.is_empty());
    assert_eq!(ids(&app, json!({"colors": "C"})).await, [azorius_win.id]);
    assert_eq!(ids(&app, json!({"colors": "junk"})).await, both);
    assert_eq!(ids(&app, json!({"color": "g"})).await, both);
    assert_eq!(
        ids(&app, json!({"winner_color": "W"})).await,
        [azorius_win.id]
    );
    assert_eq!(
        ids(&app, json!({"win_condition": "infinite_combo"})).await,
        [golgari_win.id]
    );
    assert_eq!(
        ids(&app, json!({"winner_seat": "2"})).await,
        [azorius_win.id]
    );
    assert_eq!(
        ids(&app, json!({"player_id": alice.id, "opponent_id": cara.id})).await,
        [azorius_win.id]
    );
    assert_eq!(
        ids(
            &app,
            json!({"player_id": alice.id, "player_result": "loss"})
        )
        .await,
        [azorius_win.id]
    );
    assert_eq!(
        ids(
            &app,
            json!({"player_id": alice.id, "player_result": "junk"})
        )
        .await,
        both
    );
    assert!(
        ids(&app, json!({"player_id": bob.id, "colors": "BG"}))
            .await
            .is_empty()
    );
    assert_eq!(
        ids(
            &app,
            json!({"player_id": alice.id, "colors": "BG", "player_result": "win"})
        )
        .await,
        [golgari_win.id]
    );
    assert!(
        ids(&app, json!({"player_id": bob.id, "commander": "meren"}))
            .await
            .is_empty()
    );
    assert!(
        ids(&app, json!({"winner_id": alice.id, "winner_colors": "WU"}))
            .await
            .is_empty()
    );
    assert_eq!(
        ids(
            &app,
            json!({"winner_id": bob.id, "winner_seat": 2, "winner_color": "U"})
        )
        .await,
        [azorius_win.id]
    );
    assert!(
        ids(&app, json!({"winner_seat": 1, "winner_colors": "WU"}))
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn list_games_reads_dates_weekdays_and_hours_in_the_requested_time_zone() {
    let app = TestApp::new().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let evening = app
        .simple_game("2026-09-25T01:30:00Z", alice.id, bob.id, None)
        .await;
    let afternoon = app
        .simple_game("2026-09-25T18:00:00Z", alice.id, bob.id, None)
        .await;
    let ny = "America/New_York";

    assert_eq!(
        ids(
            &app,
            json!({"date_from": "2026-09-24", "date_to": "2026-09-24", "tz": ny})
        )
        .await,
        [evening.id]
    );
    assert!(
        ids(
            &app,
            json!({"date_from": "2026-09-24", "date_to": "2026-09-24"})
        )
        .await
        .is_empty()
    );
    assert_eq!(
        ids(&app, json!({"date_from": "2026-09-25", "tz": ny})).await,
        [afternoon.id]
    );
    assert_eq!(
        ids(&app, json!({"date_from": "not-a-date"})).await,
        [afternoon.id, evening.id]
    );
    assert_eq!(
        ids(&app, json!({"weekday": "4", "tz": ny})).await,
        [evening.id]
    );
    assert_eq!(
        ids(&app, json!({"weekday": 5})).await,
        [afternoon.id, evening.id]
    );
    assert_eq!(ids(&app, json!({"hour": 21, "tz": ny})).await, [evening.id]);
    assert!(
        ids(&app, json!({"hour": "0", "weekday": 0, "tz": ny}))
            .await
            .is_empty()
    );
    assert!(
        ids(&app, json!({"hour": 14, "tz": "Not/AZone"}))
            .await
            .is_empty()
    );
    assert_eq!(
        ids(&app, json!({"hour": 18, "tz": "Not/AZone"})).await,
        [afternoon.id]
    );
}

#[tokio::test]
async fn editing_a_game_swaps_seat_numbers_and_rolls_back_invalid_edits() {
    let app = TestApp::new().await;
    let players = players(&app, 4).await;
    let refs: Vec<&Player> = players.iter().collect();
    let game = app.game(game_attrs(&refs), None).await;
    let seat_id = |player: &Player| {
        game.seats
            .iter()
            .find(|seat| seat.player_id == player.id)
            .unwrap()
            .id
    };
    let swapped = json!({"format": "two_headed_giant", "seats": [
        {"id": seat_id(&players[0]), "player_id": players[0].id, "seat": 1, "result": "win"},
        {"id": seat_id(&players[2]), "player_id": players[2].id, "seat": 2, "result": "win"},
        {"id": seat_id(&players[1]), "player_id": players[1].id, "seat": 3, "result": "loss"},
        {"id": seat_id(&players[3]).to_string(), "player_id": players[3].id, "seat": 4, "result": "loss"},
    ]});
    let updated = app.state.games.update_game(&game, &swapped).await.unwrap();
    let mut by_seat: Vec<(i64, i64)> = updated
        .seats
        .iter()
        .map(|seat| (seat.seat, seat.id))
        .collect();
    by_seat.sort_unstable();
    assert_eq!(
        by_seat,
        [
            (1, seat_id(&players[0])),
            (2, seat_id(&players[2])),
            (3, seat_id(&players[1])),
            (4, seat_id(&players[3]))
        ]
    );

    // Dropping a seat deletes it; a seat without an id is inserted.
    let fifth = app.player("Fifth").await;
    let reshaped = json!({"seats": [
        {"id": seat_id(&players[0]), "seat": 1},
        {"player_id": fifth.id, "seat": 2, "result": "loss"},
    ], "format": "commander"});
    let reshaped = app
        .state
        .games
        .update_game(&updated, &reshaped)
        .await
        .unwrap();
    assert_eq!(reshaped.seats.len(), 2);
    assert!(
        reshaped
            .seats
            .iter()
            .any(|seat| seat.player_id == fifth.id && seat.seat == 2)
    );
    assert!(
        reshaped
            .seats
            .iter()
            .any(|seat| seat.id == seat_id(&players[0]) && seat.result == GameResult::Win)
    );
}

// ColorIdentityTest

#[test]
fn canonical_reorders_letters_into_wubrg_order_and_drops_noise() {
    assert_eq!(color_identity::canonical("GRW"), "WRG");
    assert_eq!(color_identity::canonical("gW"), "W");
    assert_eq!(color_identity::canonical("WW"), "W");
    assert_eq!(color_identity::canonical(""), "");
}

#[test]
fn names_guilds_shards_wedges_four_color_and_five_color_identities() {
    for (identity, name) in [
        ("WU", "Azorius"),
        ("GB", "Golgari"),
        ("WRG", "Naya"),
        ("RGW", "Naya"),
        ("UBG", "Sultai"),
        ("WUBR", "Yore"),
        ("UBRG", "Glint"),
        ("WUBRG", "Five-Color"),
        ("R", "Mono-Red"),
        ("", "Colorless"),
    ] {
        assert_eq!(color_identity::name(identity), name);
    }
}

// DeckPickerTest

fn candidate(
    name: &str,
    skip_count: i64,
    play_count: i64,
    last_played_at: Option<&str>,
) -> Candidate {
    Candidate {
        deck: Deck {
            name: name.into(),
            skip_count,
            ..Deck::default()
        },
        play_count,
        last_played_at: last_played_at.map(utc),
        weight: 0.0,
    }
}

#[test]
fn weights_favor_never_played_older_skipped_and_less_played_decks() {
    let candidates = vec![
        candidate("Recent", 0, 1, Some("2026-09-20T11:00:00Z")),
        candidate("Old", 0, 1, Some("2026-08-20T12:00:00Z")),
        candidate("Skipped", 2, 1, Some("2026-09-20T11:00:00Z")),
        candidate("Frequent", 0, 10, Some("2026-09-20T11:00:00Z")),
        candidate("Never", 0, 0, None),
    ];
    let weights: std::collections::HashMap<String, f64> =
        selection_weights(candidates, utc("2026-09-20T12:00:00Z"))
            .into_iter()
            .map(|candidate| (candidate.deck.name, candidate.weight))
            .collect();
    assert!(weights["Never"] > weights["Old"]);
    assert!(weights["Old"] > weights["Recent"]);
    assert!(weights["Skipped"] > weights["Recent"]);
    assert!(weights["Recent"] > weights["Frequent"]);
}

async fn chooser(app: &TestApp) -> (the_gathering::accounts::User, Player) {
    let user = app.unique_member().await;
    let player = app
        .player_with(json!({"name": "Chooser"}), Some(user.id))
        .await;
    (user, player)
}

#[tokio::test]
async fn excluded_and_archived_decks_never_appear() {
    let app = TestApp::new().await;
    let (user, player) = chooser(&app).await;
    let excluded = app.deck(player.id, "Excluded", "Excluded").await;
    let archived = app
        .deck_with(json!({"player_id": player.id, "name": "Archived", "commander_name": "Archived", "archived_at": "2026-09-20T12:00:00Z"}))
        .await;
    let eligible = app.deck(player.id, "Eligible", "Eligible").await;
    sqlx::query("UPDATE decks SET included_for_play = 0 WHERE id = ?")
        .bind(excluded.id)
        .execute(app.pool())
        .await
        .unwrap();
    match app.state.games.pick_deck(&user, None, 0.0).await.unwrap() {
        the_gathering::games::DeckPick::Picked(pick) => {
            assert_eq!(pick.deck.id, eligible.id);
            assert!(![excluded.id, archived.id].contains(&pick.deck.id));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn skip_increments_and_choose_clears_the_skip_count() {
    let app = TestApp::new().await;
    let (user, player) = chooser(&app).await;
    let deck = app.deck(player.id, "Krenko", "Krenko").await;
    let games = &app.state.games;
    assert_eq!(
        games
            .record_deck_outcome(&user, deck.id, Outcome::Skipped)
            .await
            .unwrap()
            .skip_count,
        1
    );
    assert_eq!(
        games
            .record_deck_outcome(&user, deck.id, Outcome::Skipped)
            .await
            .unwrap()
            .skip_count,
        2
    );
    assert_eq!(
        games
            .record_deck_outcome(&user, deck.id, Outcome::Played)
            .await
            .unwrap()
            .skip_count,
        0
    );
}

#[tokio::test]
async fn play_count_and_last_played_are_derived_from_game_seats() {
    let app = TestApp::new().await;
    let (user, player) = chooser(&app).await;
    let deck = app.deck(player.id, "Birds", "Birds").await;
    let opponent = app.player("Opponent").await;
    app.game(
        json!({"played_at": "2026-09-19T18:00:00Z", "seats": [
            {"player_id": player.id, "deck_id": deck.id, "seat": 1, "result": "win"},
            {"player_id": opponent.id, "seat": 2, "result": "loss"},
        ]}),
        None,
    )
    .await;
    match app.state.games.pick_deck(&user, None, 0.0).await.unwrap() {
        the_gathering::games::DeckPick::Picked(pick) => {
            assert_eq!(pick.play_count, 1);
            assert_eq!(pick.last_played_at, Some(utc("2026-09-19T18:00:00Z")));
        }
        other => panic!("unexpected {other:?}"),
    }
}

// SummaryCardTest

#[test]
fn draw_six_seats_partners_missing_data_and_hostile_text_stay_bounded_and_escaped() {
    let seats = (1..=6)
        .map(|n| Seat {
            seat: n,
            result: GameResult::Draw,
            kills: (n == 1).then_some(0),
            player: Player {
                name: "<Alice & Bob>".into(),
                ..Player::default()
            },
            deck: (n == 1).then(|| Deck {
                commander_name: "Frodo".into(),
                partner_name: Some("Sam".into()),
                ..Deck::default()
            }),
            ..Seat::default()
        })
        .collect();
    let game = Game {
        id: 42,
        source: GameSource::Discord,
        external_id: Some("spellbot:SB91".into()),
        played_at: utc("2026-09-21T00:00:00Z"),
        notes: Some("A very long note & <unsafe> ".repeat(100)),
        seats,
        ..Game::default()
    };
    let svg = summary_card::svg(&game, &summary_card::Images::new());
    assert!(svg.contains("DRAW"));
    assert!(svg.contains("SB91"));
    assert!(svg.contains("Frodo / Sam"));
    assert!(svg.contains("&lt;Alice &amp; Bob&gt;"));
    assert!(!svg.contains("<unsafe>"));
    assert!(svg.contains("Commander not recorded"));
    assert!(svg.contains(">0</text>"));
    assert!(svg.contains(">—</text>"));
    assert!(svg.contains("height=\"828\""));
    assert!(svg.contains('…'));
    assert!(svg.len() < 15_000);
    assert!(summary_card::description(&game).contains("Draw"));
    // The card renders to a PNG with the bundled fonts.
    let png = the_gathering::games::summary_image::rasterize(&svg).unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
}

const JPEG: [u8; 4] = [255, 216, 255, 10];

#[tokio::test]
async fn renderer_downloads_the_scryfall_source_behind_a_catalog_image_cache_url() {
    use the_gathering::catalog::images;
    let source = "https://cards.scryfall.io/art_crop/front/0/1/01234567-89ab-cdef-0123-456789abcdef.jpg?1700000000";
    let cache_url = images::url(source);
    assert!(cache_url.contains("/api/card-images?"));
    assert_eq!(images::source(&cache_url).as_deref(), Some(source));
    assert_eq!(images::source(source).as_deref(), Some(source));
    assert_eq!(images::source("/api/card-images?other=1"), None);

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(JPEG.to_vec()))
        .mount(&server)
        .await;
    let fetcher = ArtFetcher::new(reqwest::Client::new()).with_origin(server.uri());
    // Browser-facing cache URLs are relative; only the unwrapped source passes the allowlist.
    assert_eq!(fetcher.fetch_art(Some(&cache_url)).await, None);
    let expected = format!("data:image/jpeg;base64,{}", base64_encode(&JPEG));
    assert_eq!(
        fetcher
            .fetch_art(images::source(&cache_url).as_deref())
            .await,
        Some(expected)
    );
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[tokio::test]
async fn art_fetch_only_allows_https_scryfall_raster_formats_no_redirects_capped_response_sizes() {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let plain = ArtFetcher::new(client.clone());
    for url in [
        None,
        Some("file:///etc/passwd"),
        Some("http://cards.scryfall.io/card.jpg"),
        Some("https://localhost/card.png"),
        Some("https://cards.scryfall.io.evil.test/a"),
        Some("https://user@cards.scryfall.io/a"),
        Some("https://cards.scryfall.io:8443/a"),
    ] {
        assert_eq!(plain.fetch_art(url).await, None, "{url:?}");
    }

    let mut big = vec![255, 216, 255];
    big.extend(std::iter::repeat_n(b'x', 2_000_000));
    for (status, body, accepted) in [
        (200, JPEG.to_vec(), true),
        (200, b"<svg>bad</svg>".to_vec(), false),
        (302, Vec::new(), false),
        (404, JPEG.to_vec(), false),
        (200, big, false),
    ] {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::any())
            .respond_with(
                wiremock::ResponseTemplate::new(status)
                    .insert_header("location", "https://localhost/private")
                    .set_body_bytes(body.clone()),
            )
            .mount(&server)
            .await;
        let fetcher = ArtFetcher::new(client.clone()).with_origin(server.uri());
        let result = fetcher
            .fetch_art(Some("https://cards.scryfall.io/art_crop/test.jpg"))
            .await;
        if accepted {
            assert_eq!(
                result,
                Some(format!("data:image/jpeg;base64,{}", base64_encode(&body)))
            );
        } else {
            assert_eq!(result, None, "status {status}");
        }
    }
}
