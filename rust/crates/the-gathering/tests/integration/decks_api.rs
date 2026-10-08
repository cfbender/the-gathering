//! The decks API.
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
use support::TestApp;
use the_gathering::accounts::User;
use the_gathering::games::{Deck, Player};

struct Ctx {
    app: TestApp,
    admin: User,
    owner: User,
    other: User,
    owner_player: Player,
    guest: Player,
    deck: Deck,
    guest_deck: Deck,
}

async fn setup() -> Ctx {
    let app = TestApp::new().await;
    let admin = app.unique_admin().await;
    let owner = app.unique_member().await;
    let other = app.unique_member().await;
    let owner_player = app.player("Owner").await;
    let owner_player = app
        .state
        .games
        .link_player_to_user(&owner_player, &owner)
        .await
        .unwrap();
    let guest = app.player("Guest").await;
    let deck = app.deck(owner_player.id, "Krenko", "Krenko").await;
    let guest_deck = app.deck(guest.id, "Tyvar", "Tyvar").await;
    Ctx {
        app,
        admin,
        owner,
        other,
        owner_player,
        guest,
        deck,
        guest_deck,
    }
}

async fn deck(ctx: &Ctx, id: i64) -> Option<Deck> {
    ctx.app.state.games.get_deck(id).await.unwrap()
}

#[tokio::test]
async fn another_member_cannot_edit_delete_or_create_decks_for_a_linked_player() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.other).await;
    let path = format!("/api/decks/{}", ctx.deck.id);
    assert_eq!(
        ctx.app
            .patch(&path, json!({"name": "Stolen"}))
            .await
            .assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );
    assert_eq!(deck(&ctx, ctx.deck.id).await.unwrap().name, "Krenko");
    ctx.app.delete(&path).await.assert_json(403);
    assert!(deck(&ctx, ctx.deck.id).await.is_some());
    ctx.app
        .post(
            "/api/decks",
            json!({"player_id": ctx.owner_player.id, "name": "Planted", "commander_name": "X"}),
        )
        .await
        .assert_json(403);
}

#[tokio::test]
async fn the_owner_deletes_a_deck_and_can_move_its_games_to_another_of_their_decks() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.owner).await;
    let keeper = ctx.app.deck(ctx.owner_player.id, "Keeper", "K").await;
    let game = ctx
        .app
        .game(
            json!({"played_at": "2026-09-19T18:00:00Z", "source": "manual", "seats": [
                {"player_id": ctx.owner_player.id, "deck_id": ctx.deck.id, "seat": 1, "result": "win"},
                {"player_id": ctx.guest.id, "deck_id": ctx.guest_deck.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;
    let seat_deck = || async {
        let game = ctx
            .app
            .state
            .games
            .get_game(game.id)
            .await
            .unwrap()
            .unwrap();
        game.seats
            .iter()
            .find(|seat| seat.player_id == ctx.owner_player.id)
            .unwrap()
            .deck_id
    };

    ctx.app
        .delete(&format!(
            "/api/decks/{}?replacement_deck_id={}",
            ctx.deck.id, ctx.guest_deck.id
        ))
        .await
        .assert_json(400);
    assert!(deck(&ctx, ctx.deck.id).await.is_some());

    let response = ctx
        .app
        .delete(&format!(
            "/api/decks/{}?replacement_deck_id={}",
            ctx.deck.id, keeper.id
        ))
        .await;
    assert_eq!(response.status.as_u16(), 204);
    assert!(deck(&ctx, ctx.deck.id).await.is_none());
    assert_eq!(seat_deck().await, Some(keeper.id));

    let response = ctx.app.delete(&format!("/api/decks/{}", keeper.id)).await;
    assert_eq!(response.status.as_u16(), 204);
    assert_eq!(seat_deck().await, None);
}

#[tokio::test]
async fn the_linked_member_and_administrators_can_edit_the_deck() {
    let ctx = setup().await;
    let path = format!("/api/decks/{}", ctx.deck.id);
    ctx.app.log_in(&ctx.owner).await;
    let body = ctx
        .app
        .patch(&path, json!({"name": "Krenko, Mob Boss"}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["name"], "Krenko, Mob Boss");
    assert_eq!(body["data"]["player"]["id"], ctx.owner_player.id);
    ctx.app.log_in(&ctx.admin).await;
    let body = ctx
        .app
        .patch(&path, json!({"name": "Krenko!"}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["name"], "Krenko!");
}

#[tokio::test]
async fn the_owner_can_exclude_a_deck_from_the_chooser_but_cannot_edit_its_skip_count() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.owner).await;
    let body = ctx
        .app
        .patch(
            &format!("/api/decks/{}", ctx.deck.id),
            json!({"included_for_play": false, "skip_count": 9}),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["included_for_play"], false);
    assert_eq!(body["data"]["skip_count"], 0);
    let stored = deck(&ctx, ctx.deck.id).await.unwrap();
    assert!(!stored.included_for_play);
    assert_eq!(stored.skip_count, 0);
}

#[tokio::test]
async fn the_owner_retires_a_deck_which_hides_it_from_the_list_but_keeps_it_on_the_player() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.owner).await;
    let path = format!("/api/decks/{}", ctx.deck.id);
    let body = ctx
        .app
        .patch(&path, json!({"archived_at": "2026-09-21T03:00:00.123Z"}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["archived_at"], "2026-09-21T03:00:00Z");

    let list = format!("/api/decks?player_id={}", ctx.owner_player.id);
    assert_eq!(ctx.app.get(&list).await.assert_json(200)["data"], json!([]));
    let player = ctx
        .app
        .get(&format!("/api/players/{}", ctx.owner_player.id))
        .await
        .assert_json(200);
    let decks = player["data"]["decks"].as_array().unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0]["id"], ctx.deck.id);
    assert_eq!(decks[0]["archived_at"], "2026-09-21T03:00:00Z");

    let body = ctx
        .app
        .patch(&path, json!({"archived_at": null}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["archived_at"], Value::Null);
    let listed = ctx.app.get(&list).await.assert_json(200);
    assert_eq!(listed["data"].as_array().unwrap().len(), 1);
    assert_eq!(listed["data"][0]["id"], ctx.deck.id);
    assert_eq!(listed["data"][0]["player"]["name"], "Owner");
}

#[tokio::test]
async fn members_cannot_edit_decks_of_unclaimed_guest_players() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.other).await;
    assert_eq!(
        ctx.app
            .patch(
                &format!("/api/decks/{}", ctx.guest_deck.id),
                json!({"name": "Tyvar Kell"})
            )
            .await
            .assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );
    assert_eq!(deck(&ctx, ctx.guest_deck.id).await.unwrap().name, "Tyvar");
}

#[tokio::test]
async fn patch_cannot_transfer_an_owned_or_guest_deck_and_historical_stats_remain_valid() {
    let ctx = setup().await;
    ctx.app
        .game(
            json!({"played_at": "2026-09-20T12:00:00Z", "seats": [
                {"player_id": ctx.owner_player.id, "deck_id": ctx.deck.id, "seat": 1, "result": "win"},
                {"player_id": ctx.guest.id, "seat": 2, "result": "loss"},
            ]}),
            None,
        )
        .await;
    ctx.app.log_in(&ctx.owner).await;
    let body = ctx
        .app
        .patch(
            &format!("/api/decks/{}", ctx.deck.id),
            json!({"player_id": ctx.guest.id}),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["player_id"], ctx.owner_player.id);
    assert_eq!(
        deck(&ctx, ctx.deck.id).await.unwrap().player_id,
        ctx.owner_player.id
    );
    let stats = the_gathering::stats::deck(ctx.app.pool(), ctx.deck.id, &support::input(json!({})))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stats["player"]["id"], ctx.owner_player.id);
    assert_eq!(stats["record"]["games"], 1);

    ctx.app.log_in(&ctx.other).await;
    assert_eq!(
        ctx.app
            .patch(
                &format!("/api/decks/{}", ctx.guest_deck.id),
                json!({"player_id": ctx.owner_player.id})
            )
            .await
            .assert_json(403),
        json!({"errors": {"detail": "Forbidden"}})
    );
    assert_eq!(
        deck(&ctx, ctx.guest_deck.id).await.unwrap().player_id,
        ctx.guest.id
    );
}

#[tokio::test]
async fn creating_a_deck_validates_its_fields() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.owner).await;
    let body = ctx
        .app
        .post(
            "/api/decks",
            json!({"player_id": ctx.owner_player.id, "name": " Goblins ", "commander_name": "Krenko",
                            "decklist_url": "https://www.moxfield.com/decks/abc"}),
        )
        .await
        .assert_json(201);
    assert_eq!(body["data"]["name"], "Goblins");
    assert_eq!(body["data"]["decklist_source"], "moxfield");
    assert_eq!(body["data"]["games_played"], 0);
    assert_eq!(body["data"]["player"]["name"], "Owner");

    let errors = ctx
        .app
        .post("/api/decks", json!({"name": "", "color_identity": "WW"}))
        .await
        .assert_json(422);
    assert_eq!(
        errors,
        json!({"errors": {
            "player_id": ["can't be blank"],
            "name": ["can't be blank"],
            "commander_name": ["can't be blank"],
            "color_identity": ["must contain each of W, U, B, R, and G at most once"],
        }})
    );
    let errors = ctx
        .app
        .post(
            "/api/decks",
            json!({"player_id": 999_999, "name": "x", "commander_name": "x"}),
        )
        .await;
    assert_eq!(
        errors.assert_json(422),
        json!({"errors": {"player": ["does not exist"]}})
    );
    assert_eq!(
        ctx.app.post("/api/decks", json!({})).await.assert_json(422),
        json!({"errors": {
            "player_id": ["can't be blank"],
            "name": ["can't be blank"],
            "commander_name": ["can't be blank"],
        }})
    );
    ctx.app
        .post("/api/decks", json!({"player_id": "abc"}))
        .await
        .assert_json(400);
    ctx.app
        .get("/api/decks?player_id=abc")
        .await
        .assert_json(400);
}

/// Deck links on the configured self-hosted ManaVault (`MANAVAULT_URL`, here
/// `https://manavault.example.com`) are labeled `manavault`, like manavault.app links, since
/// the deck-list resolver accepts them too.
#[tokio::test]
async fn labels_links_to_the_configured_manavault_as_manavault() {
    let ctx = setup().await;
    ctx.app.log_in(&ctx.owner).await;
    let path = format!("/api/decks/{}", ctx.deck.id);
    for (url, source) in [
        (
            "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx",
            "manavault",
        ),
        (
            "https://www.manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx",
            "manavault",
        ),
        (
            "https://app.manavault.app/share/decks/AbCdEfGhIjKlMnOpQrStUvWx",
            "manavault",
        ),
        (
            "https://vault.elsewhere.example/share/decks/AbCdEfGhIjKlMnOpQrStUvWx",
            "other",
        ),
    ] {
        let body = ctx
            .app
            .patch(&path, json!({"decklist_url": url}))
            .await
            .assert_json(200);
        assert_eq!(body["data"]["decklist_source"], source, "{url}");
    }

    let body = ctx
        .app
        .post(
            "/api/decks",
            json!({
                "player_id": ctx.owner_player.id, "name": "Vaulted", "commander_name": "Krenko",
                "decklist_url": "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx"
            }),
        )
        .await
        .assert_json(201);
    assert_eq!(body["data"]["decklist_source"], "manavault");
}

#[tokio::test]
async fn without_a_configured_manavault_only_manavault_app_is_manavault() {
    let app = TestApp::with_config(|config| config.manavault_url = None).await;
    let admin = app.unique_admin().await;
    let player = app.player("Solo").await;
    app.log_in(&admin).await;
    let body = app
        .post(
            "/api/decks",
            json!({
                "player_id": player.id, "name": "Vaulted", "commander_name": "Krenko",
                "decklist_url": "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx"
            }),
        )
        .await
        .assert_json(201);
    assert_eq!(body["data"]["decklist_source"], "other");
}
