//! Webcam table room lifecycle: reloads, crashes, closing, rematches, the shared log, idle
//! cleanup and saved sessions.
//!
//! "A seated player cannot be merged away" is covered by `WebcamTables::seated` here and
//! the merge itself in the games tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::webcam_support;

use serde_json::{Value, json};
use the_gathering::webcam::{IDLE_TIMEOUT_MS, session};
use webcam_support::{PEER_A, PEER_B, PEER_C, PEER_D, Table, peer, room_id, wait_until};

const NEW_PEER: &str = "00000000-0000-4000-8000-00000000a0a0";
const AFTER_RESTART: &str = "00000000-0000-4000-8000-00000000a0a1";
const AGAIN: &str = "00000000-0000-4000-8000-00000000a0a2";
const FRESH: &str = "00000000-0000-4000-8000-00000000f0f0";

fn without_peer(value: &Value) -> Value {
    let mut map = value.as_object().unwrap().clone();
    map.remove("peer_id");
    Value::Object(map)
}

#[tokio::test]
async fn reload_replaces_a_stale_channel_at_capacity_and_its_exit_cannot_erase_the_new_seat() {
    let mut t = Table::new().await;
    t.alice
        .ok("update_status", json!({ "life": 23, "poison": 6 }))
        .await;
    let mut seats = Vec::new();
    for index in 2..=10 {
        seats.push(t.join_seat(&peer(index)).await);
    }
    let mut replacement = t.rejoin(NEW_PEER).await;
    t.alice.expect("seat_replaced").await;
    assert_eq!(
        (
            &replacement.participant["life"],
            &replacement.participant["poison"]
        ),
        (&json!(23), &json!(6))
    );
    // The stale channel's exit leaves the new connection in place.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let debug = t.server.state().webcam_tables.debug(&t.room).await.unwrap();
    assert!(debug.connections.contains(&t.player));
    assert!(!debug.departing.contains_key(&t.player));
    assert_eq!(t.snapshot().await.seats.len(), 10);
    replacement.ok("update_status", json!({ "life": 22 })).await;
    assert_eq!(t.seat(NEW_PEER).await.life, 22);
}

#[tokio::test]
async fn a_room_outlives_its_last_seat_and_rejoining_restores_the_entire_mid_game_snapshot() {
    let mut t = Table::new().await;
    t.alice
        .ok("choose_deck", json!({ "deck_id": t.deck }))
        .await;
    t.alice
        .ok(
            "update_status",
            json!({
                "life": 17, "poison": 4, "rad": 9,
                "commander_casts": { "Kangee": 3 },
                "commander_damage": { "19": { "Atraxa": 11 } },
            }),
        )
        .await;
    t.alice.ok("take_monarch", json!({})).await;
    t.alice
        .ok("turn_settings", json!({ "auto_randomize": false }))
        .await;
    t.alice.ok("start_game", json!({})).await;
    t.alice.ok("pass_turn", json!({ "revision": 1 })).await;
    t.alice.ok("timer", json!({ "action": "pause" })).await;
    let card = json!({
        "id": "card-1", "ownerPeerId": PEER_A, "byPlayerName": "Alice", "at": 123,
        "card": { "id": "art-1", "name": "Forest", "set": "lea", "collector_number": "280" },
    });
    t.alice
        .ok("cards", json!({ "type": "card_identified", "entry": card }))
        .await;
    let before = t.snapshot().await;
    let instance = t.server.state().webcam_tables.instance(&t.room);
    t.server.disconnect(&mut t.alice, &t.room).await;
    // The room keeps running with no connections.
    assert_eq!(t.server.state().webcam_tables.instance(&t.room), instance);

    let mut rejoined = t.rejoin(AFTER_RESTART).await;
    let after = t.snapshot().await;
    assert_eq!(after.timer.timer(), before.timer.timer());
    assert_eq!(after.turns, before.turns);
    assert_eq!(after.turns.counts, [(t.player, 2)].into());
    assert_eq!(after.peer_ids, [AFTER_RESTART]);
    assert_eq!(after.monarch.holder.unwrap().peer_id, AFTER_RESTART);
    let mut moved = card.clone();
    moved["ownerPeerId"] = json!(AFTER_RESTART);
    assert_eq!(after.cards, [moved]);
    assert!(!after.auto_randomize);
    assert_eq!(
        without_peer(&rejoined.participant),
        without_peer(&serde_json::to_value(&before.seats[0]).unwrap())
    );

    rejoined
        .ok("update_status", json!({ "eliminated": true }))
        .await;
    t.disconnect(&mut rejoined).await;
    let eliminated = t.rejoin(AGAIN).await;
    assert_eq!(eliminated.participant["eliminated"], true);
    assert_eq!(eliminated.participant["life"], 17);
    let seats: Vec<String> = t
        .snapshot()
        .await
        .eliminated_seats
        .into_iter()
        .map(|seat| seat.peer_id)
        .collect();
    assert_eq!(seats, [AGAIN]);
}

#[tokio::test]
async fn a_crashing_room_stops_only_its_own_channels_which_rejoin_from_the_saved_session() {
    let mut t = Table::new().await;
    let other_room = room_id();
    let mut bystander = t.server.join_player(&other_room, PEER_B, "Bob").await;
    let tables = t.server.state().webcam_tables.clone();
    assert_ne!(tables.instance(&t.room), tables.instance(&other_room));
    t.alice.ok("update_status", json!({ "life": 21 })).await;

    tables.kill(&t.room);
    assert_eq!(
        t.alice.expect("rejoin").await,
        json!({ "reason": "room_down" })
    );

    bystander.ok("update_status", json!({ "life": 30 })).await;
    let lives: Vec<i64> = tables
        .snapshot(&other_room)
        .await
        .unwrap()
        .seats
        .iter()
        .map(|seat| seat.life)
        .collect();
    assert_eq!(lives, [30]);
    assert_eq!(t.rejoin(PEER_A).await.participant["life"], 21);
}

#[tokio::test]
async fn the_owner_ends_the_table_for_everyone_and_its_seats_leave_instead_of_rejoining() {
    let mut t = Table::new().await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    t.alice
        .ok("start_game", json!({ "randomize": false }))
        .await;
    bob.refused(
        "end_game",
        json!({}),
        "only the room owner can change table controls",
    )
    .await;
    t.alice
        .refused("end_game", json!({ "extra": true }), "invalid end game")
        .await;

    t.alice.ok("end_game", json!({})).await;
    for client in [&mut t.alice, &mut bob] {
        client.expect("table_closed").await;
        client.refute("rejoin").await;
    }
    let tables = &t.server.state().webcam_tables;
    assert_eq!(tables.instance(&t.room), None);
    assert!(!tables.rooms().iter().any(|room| room.id == t.room));
    assert!(
        session::load(t.server.app.pool(), &t.room)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_rematch_resets_the_same_room_to_a_lobby_keeping_present_seats_connected() {
    let mut t = Table::new().await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    let mut cara = t.join_player(PEER_C, "Cara").await;
    t.alice
        .ok("choose_deck", json!({ "deck_id": t.deck }))
        .await;
    t.alice.ok("set_mode", json!({ "mode": "commander" })).await;
    t.alice
        .ok(
            "arrange_seats",
            json!({ "peer_ids": [PEER_B, PEER_C, PEER_A] }),
        )
        .await;
    t.alice
        .ok("start_game", json!({ "randomize": false }))
        .await;
    let mut dave = t.join_player(PEER_D, "Dave").await;
    assert_eq!(dave.participant["spectator"], true);

    // Play a little: life, counters, a turn, an elimination, the crown, a card and a roll.
    t.alice
        .ok(
            "update_status",
            json!({ "life": 31, "poison": 3, "commander_casts": { "Kangee": 2 } }),
        )
        .await;
    bob.ok("pass_turn", json!({ "revision": 1 })).await;
    t.alice
        .ok(
            "set_eliminated",
            json!({ "peer_id": PEER_C, "eliminated": true }),
        )
        .await;
    bob.ok("take_monarch", json!({})).await;
    let card = json!({
        "id": uuid::Uuid::new_v4().to_string(), "ownerPeerId": PEER_A, "at": 1,
        "card": { "id": "art-1", "name": "Forest", "set": "lea" },
    });
    bob.ok("cards", json!({ "type": "card_identified", "entry": card }))
        .await;
    bob.ok("roll", json!({ "kind": "coin" })).await;

    // Cara leaves for good, so her seat does not carry into the new lobby.
    let cara_id = cara.player_id();
    t.server.disconnect(&mut cara, &t.room).await;
    let tables = t.server.state().webcam_tables.clone();
    let (name, token) = tables.debug(&t.room).await.unwrap().departing[&cara_id].clone();
    assert_eq!(name, "Cara");
    tables.depart(&t.room, cara_id, token);
    wait_until(|| async {
        !tables
            .debug(&t.room)
            .await
            .unwrap()
            .departing
            .contains_key(&cara_id)
    })
    .await;

    bob.refused(
        "rematch",
        json!({}),
        "only the room owner can change table controls",
    )
    .await;
    dave.refused("rematch", json!({}), "spectators cannot change the game")
        .await;
    t.alice
        .refused("rematch", json!({ "extra": true }), "invalid rematch")
        .await;
    assert_eq!(t.snapshot().await.cards.len(), 1);

    let previous_revision = t.snapshot().await.turns.revision;
    t.alice.ok("rematch", json!({})).await;
    let state = t
        .alice
        .expect_where("table_state", |state| {
            state["peer_ids"] == json!([PEER_B, PEER_A]) && state["cards"] == json!([])
        })
        .await;
    assert_eq!(state["timer"]["started_at"], Value::Null);
    assert_eq!(state["timer"]["paused_at"], Value::Null);
    assert_eq!(state["timer"]["paused_ms"], 0);
    assert_eq!(state["peer_ids"], json!([PEER_B, PEER_A]));
    assert_eq!(
        (
            &state["turns"]["active_player_id"],
            &state["turns"]["counts"],
            &state["turns"]["revision"]
        ),
        (&Value::Null, &json!({}), &json!(previous_revision + 1))
    );
    assert_eq!(state["monarch"]["holder"], Value::Null);
    assert_eq!(
        (&state["cards"], &state["eliminated_seats"]),
        (&json!([]), &json!([]))
    );
    assert_eq!(
        (&state["mode"], &state["team_life"]),
        (&json!("commander"), &json!({}))
    );
    let log = t
        .alice
        .expect_where("table_log", |log| {
            log["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["text"].as_str().unwrap().starts_with("Rematch"))
        })
        .await;
    assert_eq!(log["entries"].as_array().unwrap().len(), 1);
    assert!(
        log["entries"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Rematch: back to setup")
    );
    let entries = t.log().await;
    assert_eq!(entries.len(), 1);
    assert!(entries[0].text.starts_with("Rematch: back to setup"));

    let snapshot = t.snapshot().await;
    assert_eq!(snapshot.owner_id, t.player);
    let mut peers: Vec<&str> = snapshot
        .seats
        .iter()
        .map(|seat| seat.peer_id.as_str())
        .collect();
    peers.sort_unstable();
    assert_eq!(peers, [PEER_A, PEER_B]);
    let alice = snapshot
        .seats
        .iter()
        .find(|seat| seat.player_id == t.player)
        .unwrap();
    assert_eq!(
        (alice.life, alice.poison, alice.eliminated, alice.deck_id),
        (40, 0, false, Some(t.deck))
    );
    assert!(alice.commander_casts.is_empty());

    // Each seated connection adopts its reset seat: presence, its own state and the push.
    let reset = t.alice.expect("seat_reset").await;
    assert_eq!(
        (
            &reset["participant"]["peer_id"],
            &reset["participant"]["life"],
            &reset["participant"]["poison"]
        ),
        (&json!(PEER_A), &json!(40), &json!(0))
    );
    let reset = bob.expect("seat_reset").await;
    assert_eq!(
        (
            &reset["participant"]["peer_id"],
            &reset["participant"]["life"]
        ),
        (&json!(PEER_B), &json!(40))
    );
    let meta = t.wait_meta(PEER_A, |meta| meta["life"] == 40).await;
    assert_eq!(
        (&meta["poison"], &meta["deck_id"]),
        (&json!(0), &json!(t.deck))
    );

    // A later status change builds on the reset seat, not the old game's.
    t.alice.ok("update_status", json!({ "rad": 1 })).await;
    let alice = t.seat(PEER_A).await;
    assert_eq!((alice.life, alice.poison, alice.rad), (40, 0, 1));

    // Nobody was disconnected, spectators still watch, and the new lobby starts like any other.
    t.alice.refute("table_closed").await;
    bob.ok("update_status", json!({ "life": 38 })).await;
    dave.err("update_status", json!({ "life": 7 })).await;
    t.alice
        .ok("start_game", json!({ "randomize": false }))
        .await;
    let order = t
        .alice
        .expect_where("seat_order", |order| {
            order["peer_ids"] == json!([PEER_B, PEER_A])
        })
        .await;
    assert_eq!(order["shuffled"], false);
}

#[tokio::test]
async fn tlc_rematch_rejects_a_pass_captured_in_the_previous_game() {
    let mut t = Table::new().await;
    let _bob = t.join_player(PEER_B, "Bob").await;
    t.alice
        .ok("arrange_seats", json!({ "peer_ids": [PEER_A, PEER_B] }))
        .await;
    t.alice
        .ok("start_game", json!({ "randomize": false }))
        .await;
    let old_revision = t.snapshot().await.turns.revision;

    // TLC: Start -> CapturePass -> Rematch -> Start -> DeliverPass.
    // Delay the old request until the next game; no sleeps or timing race needed.
    t.alice.ok("rematch", json!({})).await;
    t.alice
        .ok("start_game", json!({ "randomize": false }))
        .await;
    let before = t.snapshot().await.turns;
    let (status, _) = t
        .alice
        .call("pass_turn", json!({ "revision": old_revision }))
        .await;
    let after = t.snapshot().await.turns;
    assert_eq!(
        (status.as_str(), after.active_player_id, after.counts),
        ("error", before.active_player_id, before.counts),
        "a previous game's pass must not advance the rematch"
    );
    // The guard must not disable legitimate passes or their undo in the new game.
    t.alice
        .ok("pass_turn", json!({ "revision": before.revision }))
        .await;
    let passed = t.snapshot().await.turns;
    assert_ne!(passed.active_player_id, before.active_player_id);
    t.alice
        .ok("unpass_turn", json!({ "revision": passed.revision }))
        .await;
    assert_eq!(
        t.snapshot().await.turns.active_player_id,
        before.active_player_id
    );
}

#[tokio::test]
async fn tlc_rematch_preserves_reset_life_when_an_in_flight_seat_update_arrives() {
    use the_gathering::webcam::room::{Conn, ConnEvent};
    use the_gathering::webcam::seat::Seat;
    use tokio::sync::mpsc;

    let app = crate::support::TestApp::new().await;
    let tables = &app.state.webcam_tables;
    let room = room_id();
    let mut inboxes = Vec::new();
    for (id, conn_id, peer_id, name) in [(1, 1, PEER_A, "Alice"), (2, 2, PEER_B, "Bob")] {
        let (tx, rx) = mpsc::unbounded_channel();
        tables
            .join(
                &room,
                Seat::new(peer_id.into(), id, name.into(), id),
                Conn { id: conn_id, tx },
            )
            .await
            .unwrap()
            .unwrap();
        inboxes.push(rx);
    }
    let mut bob = tables.snapshot(&room).await.unwrap().seats[1].clone();
    bob.life = 17;
    assert!(
        tables
            .remember_seat(&room, bob.clone(), 2, None)
            .await
            .unwrap()
    );

    // TLC: Damage -> PrepareSeat -> Rematch -> RememberSeat. A camera-only
    // handler has captured the old seat. Polling rematch first deterministically
    // queues the owner's command ahead of Bob's, without interrupting either.
    bob.camera_off = true;
    let (reset, update) = tokio::join!(
        biased;
        tables.rematch(&room),
        tables.remember_seat(&room, bob.clone(), 2, None),
    );
    reset.unwrap();
    assert!(!update.unwrap());

    // Reject a stale update's elimination too, not just its life/counter snapshot.
    bob.life = 0;
    assert!(
        !tables
            .remember_seat(&room, bob, 2, Some(true))
            .await
            .unwrap()
    );

    // The channel consumes its queued reset after the in-flight handler returns.
    let ConnEvent::SeatReset(reset) = inboxes[1].try_recv().unwrap() else {
        panic!("expected a rematch reset");
    };
    assert_eq!(reset.life, 40);
    let snapshot = tables.snapshot(&room).await.unwrap();
    let saved = session::load(app.pool(), &room).await.unwrap().unwrap();
    assert_eq!(
        (snapshot.seats[1].life, saved.all_seats[&2].life),
        (40, 40),
        "an in-flight camera update must not restore the previous game's life"
    );
    assert!(!snapshot.seats[1].eliminated);
    assert!(saved.eliminated_seats.is_empty());

    // The reset generation accepts new updates, including an atomic elimination.
    let mut bob = *reset;
    bob.life = 0;
    assert!(
        tables
            .remember_seat(&room, bob, 2, Some(true))
            .await
            .unwrap()
    );
    let saved = session::load(app.pool(), &room).await.unwrap().unwrap();
    assert_eq!(saved.all_seats[&2].life, 0);
    assert!(saved.all_seats[&2].eliminated);

    // A saved generation survives a room restart and another rematch.
    tables.kill(&room);
    let (tx, _rx) = mpsc::unbounded_channel();
    let joined = tables
        .join(
            &room,
            Seat::new(PEER_B.into(), 2, "Bob".into(), 2),
            Conn { id: 3, tx },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(joined.participant.generation, 1);
    tables.rematch(&room).await.unwrap();
    assert!(
        !tables
            .remember_seat(&room, joined.participant, 3, None)
            .await
            .unwrap()
    );
    let mut bob = tables.snapshot(&room).await.unwrap().seats[0].clone();
    bob.life = 29;
    assert!(tables.remember_seat(&room, bob, 3, None).await.unwrap());
    assert_eq!(tables.snapshot(&room).await.unwrap().seats[0].life, 29);
}

#[tokio::test]
async fn the_shared_log_records_seat_changes_and_rolls_and_survives_reloads_and_room_crashes() {
    let mut t = Table::new().await;
    let log = t
        .alice
        .expect_where("table_log", |log| {
            log["entries"].as_array().unwrap().len() == 1
        })
        .await;
    assert_eq!(
        (&log["entries"][0]["id"], &log["entries"][0]["text"]),
        (&json!(1), &json!("Alice joined the table"))
    );
    t.alice.ok("update_status", json!({ "life": 37 })).await;
    let entry = t.alice.expect("log_entry").await;
    assert_eq!(
        (&entry["id"], &entry["text"]),
        (&json!(2), &json!("Alice: 40 → 37 life"))
    );
    t.alice.ok("update_status", json!({ "life": 35 })).await;
    let entry = t.alice.expect("log_entry").await;
    assert_eq!(
        (&entry["id"], &entry["text"], &entry["count"]),
        (&json!(2), &json!("Alice: 40 → 35 life"), &json!(2))
    );
    t.alice.ok("roll", json!({ "kind": "coin" })).await;
    let entry = t.alice.expect("log_entry").await;
    assert_eq!(entry["id"], 3);
    assert!(
        entry["text"]
            .as_str()
            .unwrap()
            .starts_with("Alice flipped a coin: ")
    );

    // A reload within the grace period logs neither a leave nor a join.
    t.server.disconnect(&mut t.alice, &t.room).await;
    let mut reloaded = t.rejoin(NEW_PEER).await;
    let entries = reloaded.expect("table_log").await["entries"].clone();
    let texts: Vec<&str> = entries
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|e| e["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, ["Alice: 40 → 35 life", "Alice joined the table"]);

    t.server.state().webcam_tables.kill(&t.room);
    reloaded.expect("rejoin").await;

    let mut restarted = t.rejoin(AFTER_RESTART).await;
    let restored = restarted.expect("table_log").await["entries"].clone();
    let restored = restored.as_array().unwrap();
    assert_eq!(restored[0]["text"], "Alice joined the table");
    assert_eq!(restored[1..], entries.as_array().unwrap()[..]);
}

#[tokio::test]
async fn a_merge_into_an_older_log_entry_is_broadcast_in_place() {
    let mut t = Table::new().await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    t.alice
        .expect_where("log_entry", |entry| entry["text"] == "Bob joined the table")
        .await;
    t.alice.ok("update_status", json!({ "life": 37 })).await;
    let entry = t.alice.expect("log_entry").await;
    assert_eq!(
        (&entry["id"], &entry["text"]),
        (&json!(3), &json!("Alice: 40 → 37 life"))
    );
    bob.ok("update_status", json!({ "life": 38 })).await;
    let entry = t.alice.expect("log_entry").await;
    assert_eq!(
        (&entry["id"], &entry["text"]),
        (&json!(4), &json!("Bob: 40 → 38 life"))
    );
    t.alice.ok("update_status", json!({ "life": 35 })).await;
    let entry = t.alice.expect("log_entry").await;
    assert_eq!(
        (&entry["id"], &entry["text"], &entry["count"]),
        (&json!(3), &json!("Alice: 40 → 35 life"), &json!(2))
    );
    let ids: Vec<i64> = t.log().await.iter().take(2).map(|entry| entry.id).collect();
    assert_eq!(ids, [4, 3]);

    // Counter merge metadata survives a saved session.
    t.alice.ok("update_status", json!({ "poison": 2 })).await;
    let entry = t.alice.expect("log_entry").await;
    assert_eq!(
        (
            &entry["id"],
            &entry["counter"]["from"],
            &entry["counter"]["to"]
        ),
        (&json!(5), &json!(0), &json!(2))
    );
    let saved = session::load(t.server.app.pool(), &t.room)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.log, t.log().await);
}

#[tokio::test]
async fn a_seat_that_stays_away_past_the_grace_period_is_logged_as_leaving() {
    let mut t = Table::new().await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    let bob_id = bob.player_id();
    t.server.disconnect(&mut bob, &t.room).await;
    let tables = t.server.state().webcam_tables.clone();
    let (name, token) = tables.debug(&t.room).await.unwrap().departing[&bob_id].clone();
    assert_eq!(name, "Bob");

    // A stale timer from an earlier disconnect is ignored.
    tables.depart(&t.room, bob_id, token + 1000);
    t.alice
        .refute_where("log_entry", |entry| entry["text"] == "Bob left the table")
        .await;

    tables.depart(&t.room, bob_id, token);
    t.alice
        .expect_where("log_entry", |entry| entry["text"] == "Bob left the table")
        .await;
    assert_eq!(t.log().await[0].text, "Bob left the table");
}

#[tokio::test]
async fn an_empty_room_is_closed_and_its_session_deleted_only_once_idle() {
    let mut t = Table::new().await;
    t.alice.ok("update_status", json!({ "life": 3 })).await;
    let tables = t.server.state().webcam_tables.clone();

    // Connected rooms are never closed, however long they sit idle.
    assert!(tables.close_idle_rooms(0).await.is_empty());
    assert!(tables.rooms().iter().any(|room| room.id == t.room));

    t.server.disconnect(&mut t.alice, &t.room).await;
    let instance = tables.instance(&t.room);
    assert!(tables.close_idle_rooms(IDLE_TIMEOUT_MS).await.is_empty());
    assert_eq!(tables.instance(&t.room), instance);

    assert_eq!(tables.close_idle_rooms(0).await, [t.room.clone()]);
    assert!(!tables.rooms().iter().any(|room| room.id == t.room));
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM webcam_table_sessions WHERE id = ?")
        .bind(&t.room)
        .fetch_one(t.server.app.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(t.rejoin(FRESH).await.participant["life"], 40);
}

#[tokio::test]
async fn a_seat_stays_taken_until_the_table_closes() {
    let mut t = Table::new().await;
    let tables = t.server.state().webcam_tables.clone();
    assert!(tables.seated(t.player).await);
    let stranger = t.server.player("Guest", None).await;
    assert!(!tables.seated(stranger).await);

    // A departed seat still holds its place until the idle room closes.
    t.server.disconnect(&mut t.alice, &t.room).await;
    assert!(tables.seated(t.player).await);
    assert_eq!(tables.close_idle_rooms(0).await, [t.room.clone()]);
    assert!(!tables.seated(t.player).await);
}

#[tokio::test]
async fn sessions_saved_with_microsecond_expiries_by_earlier_releases_still_load_and_prune() {
    let t = Table::new().await;
    let tables = t.server.state().webcam_tables.clone();
    tables.kill(&t.room);
    let pool = t.server.app.pool();
    let set_expiry = |expires_at: String| {
        sqlx::query("UPDATE webcam_table_sessions SET expires_at = ? WHERE id = ?")
            .bind(expires_at)
            .bind(t.room.clone())
            .execute(pool)
    };
    let micros = |at: time::OffsetDateTime| {
        at.format(time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]Z"
        ))
        .unwrap()
    };
    let now = time::OffsetDateTime::now_utc();

    set_expiry(micros(now + time::Duration::days(1)))
        .await
        .unwrap();
    assert!(session::load(pool, &t.room).await.unwrap().is_some());
    assert_eq!(tables.prune_sessions().await.unwrap(), 0);

    set_expiry(micros(now - time::Duration::seconds(2)))
        .await
        .unwrap();
    assert!(session::load(pool, &t.room).await.unwrap().is_none());
    assert_eq!(tables.prune_sessions().await.unwrap(), 1);
}

#[tokio::test]
async fn expired_disconnected_sessions_are_pruned_instead_of_resurrected() {
    let mut t = Table::new().await;
    t.alice.ok("update_status", json!({ "life": 3 })).await;
    t.server.disconnect(&mut t.alice, &t.room).await;
    // A restart forgets running rooms, so the next join loads the saved session.
    let tables = t.server.state().webcam_tables.clone();
    tables.kill(&t.room);

    let pool = t.server.app.pool();
    let expired = (time::OffsetDateTime::now_utc() - time::Duration::seconds(1))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    sqlx::query("UPDATE webcam_table_sessions SET expires_at = ? WHERE id = ?")
        .bind(expired)
        .bind(&t.room)
        .execute(pool)
        .await
        .unwrap();
    assert!(session::load(pool, &t.room).await.unwrap().is_none());
    assert_eq!(tables.prune_sessions().await.unwrap(), 1);
    assert_eq!(t.rejoin(FRESH).await.participant["life"], 40);
}
