//! `POST /api/admin/games/:id/move`: moving a game into another playgroup.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use crate::support;

use serde_json::{Value, json};
use support::{PLAYGROUP, TestApp};
use the_gathering::accounts::User;
use the_gathering::games::{Deck, Player};

struct Ctx {
    app: TestApp,
    admin: User,
    crew: i64,
}

async fn setup() -> Ctx {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    app.log_in(&admin).await;
    let crew = app.playgroup("Thursday crew", None, &admin).await;
    Ctx { app, admin, crew }
}

fn path(game_id: i64) -> String {
    format!("/api/admin/games/{game_id}/move")
}

impl Ctx {
    async fn crew_player(&self, attrs: Value, user_id: Option<i64>) -> Player {
        self.app
            .state
            .games
            .create_player(self.crew, &support::input(attrs), user_id)
            .await
            .unwrap()
    }

    async fn crew_players(&self) -> Vec<Player> {
        let mut players = self
            .app
            .state
            .games
            .list_players(self.crew, true)
            .await
            .unwrap();
        players.sort_by_key(|player| player.id);
        players
    }

    async fn decks_of(&self, player_id: i64) -> Vec<Deck> {
        self.app
            .state
            .games
            .list_decks(self.crew, true, Some(player_id))
            .await
            .unwrap()
            .into_iter()
            .map(|(deck, _)| deck)
            .collect()
    }

    async fn switch_to_crew(&self) {
        self.app
            .patch("/api/session/playgroup", json!({"playgroup_id": self.crew}))
            .await
            .assert_json(200);
    }
}

#[tokio::test]
async fn moving_a_game_maps_seats_onto_the_destinations_players_and_decks() {
    let ctx = setup().await;
    let app = &ctx.app;
    let member = app.unique_member().await;
    // Alice is linked to an account, Bob to a Discord identity, Carol and Dave by name only.
    let alice = app
        .player_with(json!({"name": "Alice"}), Some(member.id))
        .await;
    let bob = app
        .player_with(json!({"name": "Bob", "discord_id": "4444"}), None)
        .await;
    let carol = app.player("Carol").await;
    let dave = app.player("Dave").await;
    let alice_deck = app
        .deck(alice.id, "Atraxa", "Atraxa, Praetors' Voice")
        .await;
    let bob_deck = app.deck(bob.id, "Krenko", "Krenko, Mob Boss").await;
    let carol_deck = app
        .deck(carol.id, "Yuriko", "Yuriko, the Tiger's Shadow")
        .await;
    let game = app
        .game(
            json!({
                "played_at": "2026-05-01T20:00:00Z",
                "seats": [
                    {"player_id": alice.id, "deck_id": alice_deck.id, "seat": 1, "result": "win",
                     "kills": 2, "mvp_card_id": "swan-song", "mvp_card_name": "Swan Song"},
                    {"player_id": bob.id, "deck_id": bob_deck.id, "seat": 2, "result": "loss",
                     "eliminated_turn": 7, "eliminated_by_player_id": alice.id},
                    {"player_id": carol.id, "deck_id": carol_deck.id, "seat": 3, "result": "loss",
                     "eliminated_turn": 9, "eliminated_by_player_id": dave.id},
                    {"player_id": dave.id, "seat": 4, "result": "loss",
                     "eliminated_turn": 10, "eliminated_by_player_id": alice.id},
                ],
            }),
            Some(ctx.admin.id),
        )
        .await;

    // In the crew: Alice's account is "Ally" with an Atraxa deck of her own, Bob's Discord
    // identity is "Bobby" without decks, and a "carol" with no identity exists by name.
    let ally = ctx
        .crew_player(json!({"name": "Ally"}), Some(member.id))
        .await;
    let ally_atraxa = app
        .deck_with(json!({"player_id": ally.id, "name": "atraxa", "commander_name": "Atraxa, Grand Unifier"}))
        .await;
    let bobby = ctx
        .crew_player(json!({"name": "Bobby", "discord_id": "4444"}), None)
        .await;
    let crew_carol = ctx.crew_player(json!({"name": "carol"}), None).await;

    let body = app
        .post(&path(game.id), json!({"playgroup_id": ctx.crew}))
        .await
        .assert_json(200);
    let seats = body["data"]["seats"].as_array().unwrap();
    assert_eq!(seats.len(), 4);
    assert_eq!(seats[0]["player_id"], ally.id);
    assert_eq!(seats[0]["deck_id"], ally_atraxa.id);
    assert_eq!(seats[0]["kills"], 2);
    assert_eq!(seats[0]["mvp_card_name"], "Swan Song");
    assert_eq!(seats[1]["player_id"], bobby.id);
    assert_eq!(seats[1]["eliminated_by_player_id"], ally.id);
    assert_eq!(seats[2]["player_id"], crew_carol.id);
    let new_dave = seats[3]["player_id"].as_i64().unwrap();
    assert_eq!(seats[2]["eliminated_by_player_id"], new_dave);
    assert_eq!(seats[3]["eliminated_by_player_id"], ally.id);
    assert_eq!(seats[3]["deck_id"], Value::Null);

    // Bob's Krenko deck and Carol's Yuriko deck were copied; Dave was created by name.
    let bobby_decks = ctx.decks_of(bobby.id).await;
    assert_eq!(bobby_decks.len(), 1);
    assert_eq!(bobby_decks[0].name, "Krenko");
    assert_eq!(bobby_decks[0].commander_name, "Krenko, Mob Boss");
    assert_eq!(seats[1]["deck_id"], bobby_decks[0].id);
    let carol_decks = ctx.decks_of(crew_carol.id).await;
    assert_eq!(carol_decks.len(), 1);
    assert_eq!(carol_decks[0].name, "Yuriko");
    assert_eq!(seats[2]["deck_id"], carol_decks[0].id);
    let crew_players = ctx.crew_players().await;
    let dave_row = crew_players
        .iter()
        .find(|player| player.id == new_dave)
        .unwrap();
    assert_eq!(dave_row.name, "Dave");
    assert_eq!(dave_row.user_id, None);
    assert_eq!(dave_row.discord_id, None);
    // Only Dave was created; nobody in the source playgroup was touched.
    assert_eq!(crew_players.len(), 4);
    assert!(
        app.state
            .games
            .get_player(PLAYGROUP, alice.id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        app.state
            .games
            .get_player_detail(PLAYGROUP, carol.id)
            .await
            .unwrap()
            .unwrap()
            .decks
            .len(),
        1
    );

    // The game left the source playgroup and shows up in the destination.
    app.get(&format!("/api/games/{}", game.id))
        .await
        .assert_json(404);
    ctx.switch_to_crew().await;
    let moved = app
        .get(&format!("/api/games/{}", game.id))
        .await
        .assert_json(200);
    assert_eq!(moved["data"]["seats"][0]["player"]["name"], "Ally");
    assert_eq!(moved["data"]["created_by_user_id"], ctx.admin.id);
}

#[tokio::test]
async fn a_name_match_takes_on_the_seats_identity_unless_it_already_has_a_different_one() {
    let ctx = setup().await;
    let app = &ctx.app;
    let member = app.unique_member().await;
    let other = app.unique_member().await;
    let alice = app
        .player_with(
            json!({"name": "Alice", "discord_id": "1111"}),
            Some(member.id),
        )
        .await;
    let bob = app
        .player_with(json!({"name": "Bob", "discord_id": "2222"}), None)
        .await;
    let crew_alice = ctx.crew_player(json!({"name": "alice"}), None).await;
    let crew_bob = ctx
        .crew_player(json!({"name": "bob", "discord_id": "9999"}), None)
        .await;
    let game = app
        .simple_game("2026-05-02T20:00:00Z", alice.id, bob.id, None)
        .await;

    // Bob's name matches a crew player with another Discord identity: refused, nothing changes.
    assert_eq!(
        app.post(&path(game.id), json!({"playgroup_id": ctx.crew}))
            .await
            .assert_json(422),
        json!({"errors": {"seats": ["bob is a different player in that playgroup"]}})
    );
    assert_eq!(ctx.crew_players().await.len(), 2);
    assert_eq!(
        app.state
            .games
            .get_game(PLAYGROUP, game.id)
            .await
            .unwrap()
            .unwrap()
            .seats[0]
            .player_id,
        alice.id
    );
    // Another account's player named differently is left alone by the move.
    ctx.crew_player(json!({"name": "Robert"}), Some(other.id))
        .await;

    // Once the crew's Bob is unlinked, the move goes through and both crew players take on
    // the identities they were missing.
    sqlx::query("UPDATE players SET discord_id = NULL WHERE id = ?")
        .bind(crew_bob.id)
        .execute(&app.state.pool)
        .await
        .unwrap();
    let body = app
        .post(&path(game.id), json!({"playgroup_id": ctx.crew}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["seats"][0]["player_id"], crew_alice.id);
    assert_eq!(body["data"]["seats"][1]["player_id"], crew_bob.id);
    let players = ctx.crew_players().await;
    let linked: Vec<(&str, Option<i64>, Option<&str>)> = players
        .iter()
        .map(|player| {
            (
                player.name.as_str(),
                player.user_id,
                player.discord_id.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        linked,
        [
            ("alice", Some(member.id), Some("1111")),
            ("bob", None, Some("2222")),
            ("Robert", Some(other.id), None),
        ]
    );
}

#[tokio::test]
async fn the_destination_must_be_another_existing_playgroup_without_the_same_imported_game() {
    let ctx = setup().await;
    let app = &ctx.app;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let game = app
        .simple_game("2026-05-03T20:00:00Z", alice.id, bob.id, None)
        .await;

    assert_eq!(
        app.post(&path(game.id), json!({"playgroup_id": PLAYGROUP}))
            .await
            .assert_json(422),
        json!({"errors": {"playgroup_id": ["is already the game's playgroup"]}})
    );
    assert_eq!(
        app.post(&path(game.id), json!({"playgroup_id": 999}))
            .await
            .assert_json(422),
        json!({"errors": {"playgroup_id": ["does not exist"]}})
    );
    app.post(&path(game.id), json!({})).await.assert_json(400);
    app.post(&path(999), json!({"playgroup_id": ctx.crew}))
        .await
        .assert_json(404);

    // The same Discord game already recorded in the crew blocks the move.
    let seats = json!({
        "played_at": "2026-05-03T21:00:00Z",
        "seats": [
            {"player_id": alice.id, "seat": 1, "result": "win"},
            {"player_id": bob.id, "seat": 2, "result": "loss"},
        ],
    });
    let imported = app
        .state
        .games
        .upsert_game_by_external_id(PLAYGROUP, "discord", "spellbot:SB7", &support::input(seats))
        .await
        .unwrap();
    let crew_alice = ctx.crew_player(json!({"name": "Alice"}), None).await;
    let crew_bob = ctx.crew_player(json!({"name": "Bob"}), None).await;
    let crew_seats = json!({
        "played_at": "2026-05-03T21:00:00Z",
        "seats": [
            {"player_id": crew_alice.id, "seat": 1, "result": "win"},
            {"player_id": crew_bob.id, "seat": 2, "result": "loss"},
        ],
    });
    app.state
        .games
        .upsert_game_by_external_id(
            ctx.crew,
            "discord",
            "spellbot:SB7",
            &support::input(crew_seats),
        )
        .await
        .unwrap();
    assert_eq!(
        app.post(&path(imported.id), json!({"playgroup_id": ctx.crew}))
            .await
            .assert_json(422),
        json!({"errors": {"playgroup_id": ["already has this game"]}})
    );
    assert_eq!(
        app.state
            .games
            .get_game(PLAYGROUP, imported.id)
            .await
            .unwrap()
            .unwrap()
            .seats[0]
            .player_id,
        alice.id
    );
}

#[tokio::test]
async fn only_admins_inside_the_sudo_window_move_games() {
    let ctx = setup().await;
    let app = &ctx.app;
    let member = app.unique_member().await;
    let alice = app.player("Alice").await;
    let bob = app.player("Bob").await;
    let game = app
        .simple_game("2026-05-04T20:00:00Z", alice.id, bob.id, Some(member.id))
        .await;
    let target = json!({"playgroup_id": ctx.crew});

    app.log_in(&member).await;
    assert_eq!(
        app.post(&path(game.id), target.clone())
            .await
            .assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );

    app.log_in(&ctx.admin).await;
    app.expire_sudo(602).await;
    assert_eq!(
        app.post(&path(game.id), target.clone())
            .await
            .assert_json(403)["errors"]["code"],
        "sudo_required"
    );

    app.expire_sudo(598).await;
    app.post(&path(game.id), target).await.assert_json(200);
    assert!(
        app.state
            .games
            .get_game(PLAYGROUP, game.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        app.state
            .games
            .get_game(ctx.crew, game.id)
            .await
            .unwrap()
            .is_some()
    );
}
