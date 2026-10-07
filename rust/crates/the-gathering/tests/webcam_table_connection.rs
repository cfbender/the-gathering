//! Ported from `test/the_gathering_web/channels/webcam_table_channel/connection_test.exs`:
//! socket authentication, SFU signaling validation, seat admission and presence/status.
//!
//! Media negotiation and ICE restarts exercise the SFU's internals and are tested with the
//! SFU crate; here signaling is checked up to the SFU boundary.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;
mod webcam_support;

use std::time::Duration;

use serde_json::{Value, json};
use the_gathering::config::{BucketLimit, WindowLimit};
use the_gathering::crypto;
use the_gathering::web::channels::{self, MAX_FRAME_SIZE};
use webcam_support::{Client, PEER_A, PEER_B, PEER_C, Server, Table, peer};

#[tokio::test]
async fn socket_connection_uses_the_tracked_cookie_session() {
    let server = Server::start().await;
    let user = server.user().await;
    let session_token = server
        .state()
        .accounts
        .generate_user_session_token(&user)
        .await
        .unwrap();
    let token = channels::socket_token(server.state(), &session_token);

    let (connected, _) = channels::authenticate(server.state(), &token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(connected.id, user.id);
    let mut client = Client::connect(server.addr, &token).await.unwrap();
    assert!(matches!(
        Client::connect(server.addr, "invalid").await,
        Err(403)
    ));

    // Logging out broadcasts the session topic, which closes its sockets.
    server.state().disconnect_session(&session_token);
    client.expect("socket_closed").await;
    assert!(client.socket_closed);

    // ...and deletes the session token, so its socket token stops working.
    server
        .state()
        .accounts
        .delete_user_session_token(&session_token)
        .await
        .unwrap();
    assert!(matches!(
        Client::connect(server.addr, &token).await,
        Err(403)
    ));
}

#[tokio::test]
async fn socket_tokens_are_encrypted_and_tampered_or_merely_signed_tokens_are_rejected() {
    let server = Server::start().await;
    let user = server.user().await;
    let session_token = server
        .state()
        .accounts
        .generate_user_session_token(&user)
        .await
        .unwrap();
    let token = channels::socket_token(server.state(), &session_token);

    assert!(!token.contains(&crypto::url_encode64_unpadded(&session_token)));
    assert!(!token.contains(crypto::url_encode64(&session_token).trim_end_matches('=')));

    // Flip a middle character; trailing base64 characters can sit in padding bits.
    let chars: Vec<char> = token.chars().collect();
    let middle = (chars.len() / 2..chars.len())
        .find(|&i| !['.', '-', '_'].contains(&chars[i]))
        .unwrap();
    let mut flipped = chars.clone();
    flipped[middle] = if chars[middle] == 'A' { 'B' } else { 'A' };
    let signed = crypto::sign(
        &session_token,
        &crypto::derive_key(
            &server.state().config.secret_key_base,
            "webcam table socket",
        ),
    );
    for tampered in [
        flipped.iter().collect::<String>(),
        chars[..middle].iter().collect(),
        signed,
    ] {
        assert!(
            channels::authenticate(server.state(), &tampered)
                .await
                .unwrap()
                .is_none(),
            "{tampered}"
        );
        assert!(matches!(
            Client::connect(server.addr, &tampered).await,
            Err(403)
        ));
    }

    // No token at all is refused before upgrading.
    let response = server.app.get("/socket/websocket?vsn=2.0.0").await;
    assert_eq!(response.status.as_u16(), 403);
}

#[tokio::test]
async fn joins_with_a_real_player_and_keeps_signaling_off_the_topic() {
    let mut t = Table::new().await;
    let presence = t.alice.expect("presence_state").await;
    assert_eq!(presence[PEER_A]["metas"][0]["player_name"], "Alice");

    let mut bob = t.join_player(PEER_B, "Bob").await;
    // Candidates go to the SFU and never fan out to the room; a malformed one is refused.
    let candidate =
        json!({ "candidate": "candidate:1 1 udp 1 127.0.0.1 9 typ host", "sdpMid": "0" });
    t.alice
        .push("sfu_candidate", json!({ "candidate": candidate }))
        .await;
    t.alice
        .refused(
            "sfu_candidate",
            json!({ "candidate": {} }),
            "invalid candidate",
        )
        .await;
    bob.refute("sfu_candidate").await;
}

#[tokio::test]
async fn rejects_oversize_or_malformed_offers_and_answers() {
    let mut t = Table::new().await;
    let oversize = "a".repeat(65_537);
    t.alice
        .refused("sfu_offer", json!({ "sdp": oversize }), "invalid offer")
        .await;
    t.alice
        .refused(
            "sfu_offer",
            json!({ "sdp": "v=0", "extra": 1 }),
            "invalid offer",
        )
        .await;
    t.alice
        .refused("sfu_offer", json!({ "sdp": "not sdp" }), "offer rejected")
        .await;
    // The server has not offered, so there is nothing to answer.
    t.alice
        .refused("sfu_answer", json!({ "sdp": "v=0\r\n" }), "answer rejected")
        .await;
    t.alice
        .refused("sfu_answer", json!({ "sdp": 1 }), "invalid answer")
        .await;
}

#[tokio::test]
async fn validates_peer_messages_without_broadcasting_them() {
    let mut t = Table::new().await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    t.alice
        .expect_where("presence_diff", |diff| diff["joins"].get(PEER_B).is_some())
        .await;

    t.alice
        .refused(
            "peer_message",
            json!({ "to": PEER_A, "message": {} }),
            "invalid recipient",
        )
        .await;
    t.alice
        .refused(
            "peer_message",
            json!({ "to": "peer-b", "message": {} }),
            "invalid recipient",
        )
        .await;
    t.alice
        .refused(
            "peer_message",
            json!({ "to": PEER_C, "message": {} }),
            "recipient has left",
        )
        .await;
    t.alice
        .refused(
            "peer_message",
            json!({ "to": PEER_B, "message": "crop" }),
            "invalid message",
        )
        .await;
    let oversize = json!({ "data": "a".repeat(262_145) });
    t.alice
        .refused(
            "peer_message",
            json!({ "to": PEER_B, "message": oversize }),
            "message too large",
        )
        .await;
    bob.refute("peer_message").await;
}

#[tokio::test]
async fn validates_layer_requests_against_the_known_layers_and_seats() {
    let mut t = Table::new().await;
    t.alice
        .refused(
            "sfu_layer",
            json!({ "peer_id": PEER_B, "layer": "xl" }),
            "invalid layer",
        )
        .await;
    t.alice
        .refused(
            "sfu_layer",
            json!({ "peer_id": "peer-b", "layer": "l" }),
            "invalid layer",
        )
        .await;
    t.alice
        .refused(
            "sfu_layer",
            json!({ "peer_id": PEER_B, "layer": "l" }),
            "unknown board",
        )
        .await;
}

#[tokio::test]
async fn the_websocket_caps_inbound_frames_above_the_largest_legitimate_signal() {
    const { assert!(MAX_FRAME_SIZE > 65_536) };
    let mut t = Table::new().await;
    // A card crop just over the relay limit still fits in a frame and is refused politely.
    let crop = json!({ "data": "a".repeat(262_145) });
    t.alice
        .refused(
            "peer_message",
            json!({ "to": PEER_B, "message": crop }),
            "message too large",
        )
        .await;
    // Anything above the frame cap closes the socket.
    let huge = json!({ "data": "a".repeat(MAX_FRAME_SIZE) });
    t.alice
        .push("peer_message", json!({ "to": PEER_B, "message": huge }))
        .await;
    t.alice.expect("socket_closed").await;
}

#[tokio::test]
async fn updates_presence_only_with_a_deck_owned_by_the_seated_player() {
    let mut t = Table::new().await;
    t.alice
        .ok("choose_deck", json!({ "deck_id": t.deck }))
        .await;
    assert_eq!(
        t.alice.expect("deck_selected").await,
        json!({ "deck_id": t.deck })
    );
    assert_eq!(t.meta(PEER_A)["deck_id"], t.deck);

    // Reselecting after an art/partner edit must refresh peer caches even with the same id.
    t.alice
        .ok("choose_deck", json!({ "deck_id": t.deck }))
        .await;
    assert_eq!(
        t.alice.expect("deck_selected").await,
        json!({ "deck_id": t.deck })
    );

    let other = t.server.player("Bob", None).await;
    let other_deck = t.server.deck(other, "Dragons", "Miirym").await;
    t.alice
        .refused(
            "choose_deck",
            json!({ "deck_id": other_deck }),
            "deck does not belong to player",
        )
        .await;
    t.alice
        .refused("choose_deck", json!({ "deck_id": "1" }), "invalid deck")
        .await;
    t.alice.refute("deck_selected").await;
}

#[tokio::test]
async fn publishes_life_and_camera_status_through_presence() {
    let mut t = Table::new().await;
    let presence = t.alice.expect("presence_state").await;
    let meta = &presence[PEER_A]["metas"][0];
    assert_eq!(
        (meta["life"].clone(), meta["camera_off"].clone()),
        (json!(40), json!(false))
    );
    assert!(meta["joined_at"].is_i64());

    t.alice
        .ok("update_status", json!({ "life": 37, "camera_off": true }))
        .await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (meta["life"].clone(), meta["camera_off"].clone()),
        (json!(37), json!(true))
    );
    assert!(meta.get("muted").is_none());

    t.alice
        .refused("update_status", json!({ "life": 1_000 }), "invalid status")
        .await;
    t.alice
        .refused(
            "update_status",
            json!({ "life": 20, "role": "admin" }),
            "invalid status",
        )
        .await;
    t.alice
        .refused("update_status", json!([1]), "invalid status")
        .await;
    assert_eq!(t.meta(PEER_A)["life"], 37);
}

#[tokio::test]
async fn publishes_the_cameras_native_height_and_correction_consent() {
    let mut t = Table::new().await;
    let presence = t.alice.expect("presence_state").await;
    let meta = &presence[PEER_A]["metas"][0];
    assert_eq!(
        (
            meta["camera_height"].clone(),
            meta["shares_corrections"].clone()
        ),
        (Value::Null, json!(false))
    );

    t.alice
        .ok(
            "update_status",
            json!({ "camera_height": 1080, "shares_corrections": true }),
        )
        .await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (
            meta["camera_height"].clone(),
            meta["shares_corrections"].clone()
        ),
        (json!(1080), json!(true))
    );

    for bad in [
        json!({ "camera_height": 0 }),
        json!({ "camera_height": "1080" }),
        json!({ "shares_corrections": 1 }),
    ] {
        t.alice
            .refused("update_status", bad, "invalid status")
            .await;
    }

    // The placeholder stream has no camera behind it.
    t.alice
        .ok("update_status", json!({ "camera_height": null }))
        .await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (
            meta["camera_height"].clone(),
            meta["shares_corrections"].clone()
        ),
        (Value::Null, json!(true))
    );
}

#[tokio::test]
async fn a_full_status_update_right_after_joining_is_applied() {
    let mut t = Table::new().await;
    let full = json!({ "life": 33, "poison": 1, "rad": 2, "commander_casts": {}, "commander_damage": {}, "camera_off": true });
    t.alice.ok("update_status", full).await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (meta["life"].clone(), meta["camera_off"].clone()),
        (json!(33), json!(true))
    );
    assert_eq!(
        t.snapshot()
            .await
            .seats
            .iter()
            .map(|seat| seat.life)
            .collect::<Vec<_>>(),
        [33]
    );
}

#[tokio::test]
async fn rejects_invalid_room_ids() {
    let t = Table::new().await;
    let user = t.server.user().await;
    let (status, response, _) = t
        .server
        .try_join(&user, t.player, "not-a-uuid", PEER_B)
        .await;
    assert_eq!(
        (status.as_str(), response),
        ("error", json!({ "reason": "invalid room" }))
    );
}

#[tokio::test]
async fn admits_ten_seats_marks_the_lobby_full_and_refuses_the_eleventh() {
    let mut t = Table::new().await;
    let full = |state: &the_gathering::state::AppState, room: &str| {
        the_gathering::web::channels::rooms::active_rooms(state)
            .into_iter()
            .find(|r| r.id == room)
            .unwrap()
            .full
    };
    let mut seats = Vec::new();
    for index in 2..=9 {
        seats.push(t.join_seat(&peer(index)).await);
    }
    assert!(!full(t.server.state(), &t.room));
    seats.push(t.join_seat(&peer(10)).await);
    assert_eq!(t.server.state().presence.count(&t.topic()), 10);
    assert!(full(t.server.state(), &t.room));

    let (user, player) = t.server.linked_player(&peer(11)).await;
    let (status, response, _) = t.server.try_join(&user, player, &t.room, &peer(11)).await;
    assert_eq!(
        (status.as_str(), response),
        ("error", json!({ "reason": "room is full" }))
    );

    let order: Vec<String> = std::iter::once(PEER_A.to_owned())
        .chain((2..=10).map(peer))
        .rev()
        .collect();
    t.alice.ok("seat_order", json!({ "peer_ids": order })).await;
    assert_eq!(t.alice.expect("seat_order").await["peer_ids"], json!(order));
}

#[tokio::test]
async fn rejects_non_uuid_and_duplicate_peer_ids() {
    let t = Table::new().await;
    let (user, player) = t.server.linked_player("Bob").await;
    let long = "a".repeat(10_000);
    let upper = PEER_B.to_uppercase();
    for (peer_id, reason) in [
        (json!(""), "invalid peer id"),
        (json!("peer-b"), "invalid peer id"),
        (json!(upper), "invalid peer id"),
        (json!(long), "invalid peer id"),
        (json!(123), "invalid peer id"),
        (json!(PEER_A), "peer id is already in use"),
    ] {
        let mut client = t.server.connect(&user).await;
        let (status, response) = client
            .join(
                &t.topic(),
                json!({ "peer_id": peer_id, "player_id": player }),
            )
            .await;
        assert_eq!(
            (status.as_str(), response),
            ("error", json!({ "reason": reason })),
            "{peer_id}"
        );
    }
    assert_eq!(t.server.state().presence.count(&t.topic()), 1);
}

#[tokio::test]
async fn each_connections_events_are_limited_with_signals_budgeted_separately() {
    let mut t = Table::with_config(|config| {
        config.rate_limits.webcam_table_events = BucketLimit {
            capacity: 3.0,
            refill_per_second: 0.0,
        };
        config.rate_limits.webcam_table_signals = BucketLimit {
            capacity: 2.0,
            refill_per_second: 0.0,
        };
    })
    .await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    for life in [39, 38, 37] {
        bob.ok("update_status", json!({ "life": life })).await;
    }
    bob.refused("update_status", json!({ "life": 1 }), "rate limited")
        .await;
    bob.refused("roll", json!({ "kind": "coin" }), "rate limited")
        .await;
    assert_eq!(t.seat(PEER_B).await.life, 37);

    // Signals spend their own bucket (relayed messages answer only on failure).
    for _ in 0..2 {
        bob.push(
            "peer_message",
            json!({ "to": PEER_A, "message": { "type": "crop" } }),
        )
        .await;
    }
    bob.settle(Duration::from_millis(100)).await;
    bob.refused(
        "peer_message",
        json!({ "to": PEER_A, "message": {} }),
        "rate limited",
    )
    .await;

    // Alice has her own budget.
    t.alice.ok("update_status", json!({ "life": 12 })).await;
}

#[tokio::test]
async fn joins_are_limited_per_user_so_rejoining_cannot_reset_the_budget() {
    let server = Server::with_config(|config| {
        config.rate_limits.webcam_table_joins = WindowLimit {
            limit: 1,
            scale: Duration::from_secs(60),
        };
    })
    .await;
    let (user, player) = server.linked_player("Bob").await;
    let room = webcam_support::room_id();
    let mut joined = server.join_as(&user, player, &room, PEER_B).await;
    joined.leave().await;
    let (status, response, _) = server.try_join(&user, player, &room, PEER_B).await;
    assert_eq!(
        (status.as_str(), response),
        ("error", json!({ "reason": "rate limited" }))
    );
}

#[tokio::test]
async fn rejects_a_player_not_linked_to_the_authenticated_account() {
    let t = Table::new().await;
    let user = t.server.user().await;
    let (status, response, _) = t.server.try_join(&user, t.player, &t.room, PEER_B).await;
    assert_eq!(
        (status.as_str(), response),
        (
            "error",
            json!({ "reason": "account is not linked to this player" })
        )
    );
    let mut client = t.server.connect(&user).await;
    let (status, response) = client
        .join(&t.topic(), json!({ "peer_id": PEER_B, "player_id": "1" }))
        .await;
    assert_eq!(
        (status.as_str(), response),
        (
            "error",
            json!({ "reason": "account is not linked to a player" })
        )
    );
}

#[tokio::test]
async fn answers_heartbeats_and_unmatched_topics() {
    let t = Table::new().await;
    let mut client = t.server.connect(&t.user).await;
    client
        .send(None, "1", "phoenix", "heartbeat", json!({}))
        .await;
    assert_eq!(client.reply("1").await, ("ok".to_owned(), json!({})));
    client
        .send(Some("2"), "2", "rooms:lobby", "phx_join", json!({}))
        .await;
    assert_eq!(
        client.reply("2").await,
        ("error".to_owned(), json!({ "reason": "unmatched topic" }))
    );
}
