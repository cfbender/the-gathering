//! Webcam table state: counters, monarch, seat order, reveal, timers, spectators, rolls,
//! elimination, turns and identified cards.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::webcam_support;

use serde_json::{Value, json};
use the_gathering::webcam::room::Actor;
use webcam_support::{PEER_A, PEER_B, PEER_C, Table, room_id};

const PEER_A_NEW: &str = "00000000-0000-4000-8000-0000000000a2";
const ELSEWHERE: &str = "00000000-0000-4000-8000-00000000e0e0";

fn with(base: &Value, key: &str, value: Value) -> Value {
    let mut map = base.as_object().unwrap().clone();
    map.insert(key.into(), value);
    Value::Object(map)
}

fn without(base: &Value, key: &str) -> Value {
    let mut map = base.as_object().unwrap().clone();
    map.remove(key);
    Value::Object(map)
}

fn drop_server_now(timer: &Value) -> Value {
    without(timer, "server_now")
}

#[tokio::test]
async fn publishes_separate_commander_counters_and_rejects_invalid_updates_atomically() {
    let mut t = Table::new().await;
    let presence = t.alice.expect("presence_state").await;
    let initial = &presence[PEER_A]["metas"][0];
    assert_eq!(
        (
            &initial["poison"],
            &initial["rad"],
            &initial["commander_casts"],
            &initial["commander_damage"]
        ),
        (&json!(0), &json!(0), &json!({}), &json!({}))
    );
    let damage = json!({ "2": { "Tymna": 21, "Thrasios": 4 }, "3": { "Tymna": 8 } });
    let casts = json!({ "Tymna": 3, "Thrasios": 1 });
    t.alice
        .ok(
            "update_status",
            json!({ "poison": 10, "rad": 3, "commander_damage": damage, "commander_casts": casts }),
        )
        .await;
    t.alice.ok("update_status", json!({ "life": 37 })).await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (&meta["poison"], &meta["rad"], &meta["life"]),
        (&json!(10), &json!(3), &json!(37))
    );
    assert_eq!(meta["commander_damage"], damage);
    assert_eq!(meta["commander_casts"], casts);

    for payload in [
        json!({ "poison": -1 }),
        json!({ "rad": 1.5 }),
        json!({ "poison": "2" }),
        json!({ "rad": 1000 }),
        json!({ "commander_casts": { "Tymna": -1 } }),
        json!({ "commander_casts": { "": 2 } }),
        json!({ "commander_casts": [] }),
        json!({ "commander_damage": { "2": { "Tymna": 1.5 } } }),
        json!({ "commander_damage": { "peer-ghost": { "Tymna": 1 } } }),
        json!({ "commander_damage": { "2": 3 } }),
        json!({ "monarch": true }),
        json!({ "peer_id": PEER_B }),
    ] {
        t.alice
            .refused(
                "update_status",
                with(&payload, "life", json!(1)),
                "invalid status",
            )
            .await;
    }
    let unchanged = t.meta(PEER_A);
    assert_eq!(unchanged["life"], 37);
    assert_eq!(unchanged["commander_damage"], damage);

    t.alice
        .ok(
            "update_status",
            json!({ "poison": 0, "rad": 999, "commander_casts": {} }),
        )
        .await;
}

#[tokio::test]
async fn publishes_shared_custom_counters_and_combat_buffs_rejecting_malformed_ones() {
    let mut t = Table::new().await;
    let presence = t.alice.expect("presence_state").await;
    let initial = &presence[PEER_A]["metas"][0];
    assert_eq!(
        (&initial["custom_counters"], &initial["combat_effects"]),
        (&json!([]), &json!([]))
    );

    let counters = json!([{ "id": "c1", "label": "Lands", "value": 7 }]);
    let effect = json!({
        "id": "e1", "name": "Intangible Virtue", "power": 1, "toughness": 1,
        "conditions": ["Token"], "keywords": ["vigilance"],
    });
    let effects = json!([effect]);
    t.alice
        .ok(
            "update_status",
            json!({ "custom_counters": counters, "combat_effects": effects }),
        )
        .await;
    let meta = t.meta(PEER_A);
    assert_eq!(meta["custom_counters"], counters);
    assert_eq!(meta["combat_effects"], effects);
    let seats = serde_json::to_value(t.snapshot().await.seats).unwrap();
    assert_eq!(seats[0]["custom_counters"], counters);
    assert!(
        t.log()
            .await
            .iter()
            .any(|entry| entry.text == "Alice Lands: 0 → 7")
    );

    for payload in [
        json!({ "custom_counters": {} }),
        json!({ "custom_counters": [{ "id": "c1", "label": "Lands", "value": 101 }] }),
        json!({ "custom_counters": [{ "id": "c1", "label": "", "value": 1 }] }),
        json!({ "custom_counters": [{ "id": "c1", "label": "Lands", "value": 1, "x": 1 }] }),
        json!({ "custom_counters": [{ "label": "Lands", "value": 1 }] }),
        json!({ "combat_effects": [with(&effect, "power", json!(100))] }),
        json!({ "combat_effects": [with(&effect, "toughness", json!("1"))] }),
        json!({ "combat_effects": [with(&effect, "conditions", json!("Token"))] }),
        json!({ "combat_effects": [with(&effect, "keywords", json!([""]))] }),
        json!({ "combat_effects": [without(&effect, "keywords")] }),
        json!({ "combat_effects": [with(&effect, "shared", json!(true))] }),
    ] {
        t.alice
            .refused("update_status", payload, "invalid status")
            .await;
    }
    let unchanged = t.meta(PEER_A);
    assert_eq!(unchanged["custom_counters"], counters);
    assert_eq!(unchanged["combat_effects"], effects);

    t.alice
        .ok(
            "update_status",
            json!({ "custom_counters": [], "combat_effects": [] }),
        )
        .await;
}

fn holder_is(peer: &'static str) -> impl Fn(&Value) -> bool {
    move |payload| payload["holder"]["peer_id"] == peer
}

#[tokio::test]
async fn monarch_is_one_shared_holder_synchronized_to_late_joiners_and_retained_on_departure() {
    let mut t = Table::new().await;
    assert_eq!(t.alice.expect("monarch_state").await["holder"], Value::Null);
    t.alice
        .err("take_monarch", json!({ "peer_id": "other" }))
        .await;
    t.alice.ok("take_monarch", json!({})).await;
    assert_eq!(
        t.alice.expect("monarch").await["holder"],
        json!({ "peer_id": PEER_A, "player_name": "Alice" })
    );

    let mut bob = t.join_player(PEER_B, "Bob").await;
    assert_eq!(
        bob.expect("monarch_state").await["holder"]["peer_id"],
        PEER_A
    );
    bob.ok("take_monarch", json!({})).await;
    let bob_holder = json!({ "peer_id": PEER_B, "player_name": "Bob" });
    assert_eq!(t.alice.expect("monarch").await["holder"], bob_holder);
    assert_eq!(bob.expect("monarch").await["holder"], bob_holder);
    bob.ok("take_monarch", json!({})).await;
    bob.refute("monarch").await;
    t.alice.refute("monarch").await;

    // Any player may hand the crown to another seated player, and back.
    bob.err("take_monarch", json!({ "peer_id": ELSEWHERE }))
        .await;
    bob.refused(
        "take_monarch",
        json!({ "peer_id": PEER_A, "x": 1 }),
        "invalid monarch claim",
    )
    .await;
    bob.ok("take_monarch", json!({ "peer_id": PEER_A })).await;
    t.alice.expect_where("monarch", holder_is(PEER_A)).await;
    bob.expect_where("monarch", holder_is(PEER_A)).await;
    bob.expect_where("log_entry", |entry| {
        entry["text"] == "Bob gave Alice the monarch"
    })
    .await;

    t.alice
        .ok("take_monarch", json!({ "peer_id": PEER_B }))
        .await;
    t.alice.expect_where("monarch", holder_is(PEER_B)).await;
    bob.expect_where("monarch", holder_is(PEER_B)).await;

    // The previous holder leaving must not clear Bob's crown.
    t.alice.leave().await;
    bob.refute_where("monarch", |payload| payload["holder"].is_null())
        .await;
    let mut cara = t.join_player(PEER_C, "Cara").await;
    assert_eq!(
        cara.expect("monarch_state").await["holder"]["peer_id"],
        PEER_B
    );
    bob.leave().await;
    cara.refute_where("monarch", |payload| payload["holder"].is_null())
        .await;
    assert_eq!(t.snapshot().await.monarch.holder.unwrap().peer_id, PEER_B);
}

#[tokio::test]
async fn concurrent_monarch_claims_converge_on_the_last_serialized_event() {
    let mut t = Table::new().await;
    t.alice.expect("monarch_state").await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    bob.expect("monarch_state").await;
    let alice_ref = t.alice.push("take_monarch", json!({})).await;
    let bob_ref = bob.push("take_monarch", json!({})).await;
    assert_eq!(t.alice.reply(&alice_ref).await.0, "ok");
    assert_eq!(bob.reply(&bob_ref).await.0, "ok");
    let first = t.alice.expect("monarch").await;
    let last = t.alice.expect("monarch").await;
    assert!(first["revision"].as_i64() < last["revision"].as_i64());
    let mut holders = [
        first["holder"]["peer_id"].clone(),
        last["holder"]["peer_id"].clone(),
    ];
    holders.sort_by_key(ToString::to_string);
    assert_eq!(holders, [json!(PEER_A), json!(PEER_B)]);
    // Every seat sees the same order.
    assert_eq!(bob.expect("monarch").await, first);
    assert_eq!(bob.expect("monarch").await, last);

    let mut cara = t.join_player(PEER_C, "Cara").await;
    let state = cara.expect("monarch_state").await;
    assert_eq!(state, last);
}

#[tokio::test]
async fn broadcasts_a_seat_order_that_names_every_present_peer() {
    let mut t = Table::new().await;
    t.alice
        .ok("seat_order", json!({ "peer_ids": [PEER_A] }))
        .await;
    assert_eq!(
        t.alice.expect("seat_order").await["peer_ids"],
        json!([PEER_A])
    );
    t.alice
        .refused(
            "seat_order",
            json!({ "peer_ids": [PEER_A, "peer-ghost"] }),
            "seat order must list every seated player",
        )
        .await;
    t.alice
        .refused(
            "seat_order",
            json!({ "peer_ids": PEER_A }),
            "invalid seat order",
        )
        .await;
}

#[tokio::test]
async fn reveal_validates_targets_preserves_status_and_ends_when_the_target_leaves() {
    let mut t = Table::new().await;
    let mut other = t.join_seat(PEER_B).await;
    t.alice.ok("reveal", json!({ "target": PEER_B })).await;
    t.alice.ok("update_status", json!({ "life": 31 })).await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (&meta["reveal_to"], &meta["life"]),
        (&json!(PEER_B), &json!(31))
    );

    for target in [PEER_A, "absent", ""] {
        t.alice.err("reveal", json!({ "target": target })).await;
    }
    for payload in [
        json!({ "target": 123 }),
        json!({}),
        json!({ "target": null, "peer_id": PEER_B }),
    ] {
        t.alice.err("reveal", payload).await;
    }
    t.alice
        .err("update_status", json!({ "reveal_to": PEER_B }))
        .await;
    t.alice.ok("reveal", json!({ "target": null })).await;
    assert_eq!(t.meta(PEER_A)["reveal_to"], Value::Null);
    t.alice.ok("reveal", json!({ "target": PEER_B })).await;
    t.alice.drain();

    other.leave().await;
    t.alice
        .expect_where("presence_diff", |diff| {
            diff["joins"][PEER_A]["metas"][0].get("reveal_to") == Some(&Value::Null)
        })
        .await;
    t.alice
        .expect_where("presence_diff", |diff| diff["leaves"].get(PEER_B).is_some())
        .await;
    assert_eq!(t.meta(PEER_A)["reveal_to"], Value::Null);
}

#[tokio::test]
async fn server_timestamps_start_pause_and_resume_and_reordering_preserves_the_timer() {
    let mut t = Table::new().await;
    let state = t.alice.expect("table_state").await;
    assert_eq!(
        (&state["timer"]["started_at"], &state["peer_ids"]),
        (&Value::Null, &json!([]))
    );
    let before_start = the_gathering::webcam::now();
    t.alice
        .ok("seat_order", json!({ "peer_ids": [PEER_A] }))
        .await;
    // Starting holds the clock at zero for mulligans until the first player begins play.
    let timer = t.alice.expect("timer_state").await;
    let started = timer["started_at"].as_i64().unwrap();
    assert_eq!(timer["paused_at"], started);
    assert_eq!(timer["paused_ms"], 0);
    assert!(started >= before_start && started <= the_gathering::webcam::now());
    let running = t.alice.ok("begin_play", json!({})).await;
    assert_eq!(
        (&running["started_at"], &running["paused_at"]),
        (&json!(started), &Value::Null)
    );

    let paused = t.alice.ok("timer", json!({ "action": "pause" })).await;
    assert_eq!(paused["started_at"], started);
    assert!(paused["paused_at"].is_i64());
    t.alice
        .ok("seat_order", json!({ "peer_ids": [PEER_A] }))
        .await;
    let still_paused = t.alice.ok("timer_sync", json!({})).await;
    assert_eq!(still_paused["paused_at"], paused["paused_at"]);
    assert_eq!(still_paused["started_at"], started);

    let resumed = t.alice.ok("timer", json!({ "action": "resume" })).await;
    assert_eq!(
        (&resumed["paused_at"], &resumed["started_at"]),
        (&Value::Null, &json!(started))
    );
    assert!(resumed["paused_ms"].as_i64().unwrap() >= 0);
    let repeated = t.alice.ok("timer", json!({ "action": "resume" })).await;
    assert_eq!(repeated["paused_ms"], resumed["paused_ms"]);
}

#[tokio::test]
async fn late_arrivals_receive_state_as_spectators_and_cannot_mutate_it() {
    let mut t = Table::new().await;
    t.alice
        .ok("seat_order", json!({ "peer_ids": [PEER_A] }))
        .await;
    let timer = t.alice.ok("timer", json!({ "action": "pause" })).await;
    let mut other = t.join_player(PEER_B, "Bob").await;
    let state = other.expect("table_state").await;
    assert_eq!(state["timer"]["paused_at"], timer["paused_at"]);
    assert_eq!(state["peer_ids"], json!([PEER_A]));
    assert_eq!(drop_server_now(&state["timer"]), drop_server_now(&timer));
    assert_eq!(other.participant["spectator"], true);
    assert_eq!(
        t.snapshot()
            .await
            .seats
            .iter()
            .map(|seat| seat.peer_id.as_str())
            .collect::<Vec<_>>(),
        [PEER_A]
    );

    for (event, payload) in [
        ("timer", json!({ "action": "resume" })),
        ("update_status", json!({ "life": 7 })),
        ("start_game", json!({})),
        ("pass_turn", json!({ "revision": 1 })),
        ("unpass_turn", json!({ "revision": 1 })),
        ("take_monarch", json!({})),
        (
            "set_eliminated",
            json!({ "peer_id": PEER_A, "eliminated": true }),
        ),
        (
            "cards",
            json!({ "type": "cards_cleared", "ownerPeerId": PEER_A }),
        ),
    ] {
        other
            .refused(event, payload, "spectators cannot change the game")
            .await;
    }

    let synced = other.ok("timer_sync", json!({})).await;
    assert_eq!(synced["paused_at"], timer["paused_at"]);
    let resumed = t.alice.ok("timer", json!({ "action": "resume" })).await;
    assert_eq!(
        (&resumed["started_at"], &resumed["paused_at"]),
        (&timer["started_at"], &Value::Null)
    );
    let broadcast = t
        .alice
        .expect_where("timer_state", |timer| timer["paused_at"].is_null())
        .await;
    assert_eq!(broadcast["started_at"], timer["started_at"]);

    let mut cara = t.server.join_player(&room_id(), PEER_C, "Cara").await;
    let state = cara.expect("table_state").await;
    assert_eq!(
        (&state["timer"]["started_at"], &state["timer"]["paused_ms"]),
        (&Value::Null, &json!(0))
    );
    assert_eq!(state["peer_ids"], json!([]));
}

#[tokio::test]
async fn only_the_first_player_or_the_owner_can_end_the_mulligan_window() {
    let mut t = Table::new().await;
    let mut other = t.join_player(PEER_B, "Bob").await;
    assert_eq!(
        t.alice.ok("begin_play", json!({})).await["started_at"],
        Value::Null
    );
    t.alice
        .ok("turn_settings", json!({ "auto_randomize": false }))
        .await;
    t.alice
        .ok("arrange_seats", json!({ "peer_ids": [PEER_B, PEER_A] }))
        .await;
    t.alice.ok("start_game", json!({})).await;
    let bob = other.player_id();
    assert_eq!(t.snapshot().await.turns.active_player_id, Some(bob));

    // Bob goes first, so Alice acting as a plain seat (not as owner) is refused.
    let refused = t
        .server
        .state()
        .webcam_tables
        .begin_play(&t.room, Actor::Player(t.player))
        .await
        .unwrap();
    assert_eq!(
        refused,
        Err("only the first player can start the game".into())
    );

    t.alice
        .refused("begin_play", json!({ "at": 1 }), "invalid start")
        .await;
    let running = other.ok("begin_play", json!({})).await;
    assert_eq!(running["paused_at"], Value::Null);
    t.alice
        .expect_where("timer_state", |timer| timer["paused_at"].is_null())
        .await;

    // Once running, repeated starts change nothing.
    let again = t.alice.ok("begin_play", json!({})).await;
    assert_eq!(drop_server_now(&again), drop_server_now(&running));
}

#[tokio::test]
async fn rejects_forged_timestamps_unknown_timer_actions_and_invalid_sync_payloads() {
    let mut t = Table::new().await;
    for payload in [
        json!({ "action": "start" }),
        json!({ "action": "reset" }),
        json!({}),
        json!({ "action": "pause", "started_at": 1 }),
    ] {
        t.alice
            .refused("timer", payload, "invalid timer action")
            .await;
    }
    t.alice.err("timer_sync", json!({ "server_now": 1 })).await;
    assert_eq!(
        t.alice.ok("timer_sync", json!({})).await["started_at"],
        Value::Null
    );
}

#[tokio::test]
async fn server_generates_attributed_dice_and_coin_rolls_rejecting_forged_or_invalid_payloads() {
    let mut t = Table::new().await;
    for sides in [2, 6, 20, 1000] {
        t.alice
            .ok("roll", json!({ "kind": "dice", "sides": sides }))
            .await;
        let roll = t.alice.expect("roll").await;
        assert_eq!(
            (&roll["kind"], &roll["sides"]),
            (&json!("dice"), &json!(sides))
        );
        assert_eq!(
            (&roll["actor"], &roll["player_name"]),
            (&json!(PEER_A), &json!("Alice"))
        );
        assert!((1..=sides).contains(&roll["result"].as_i64().unwrap()));
        assert!(roll["at"].is_i64());
        assert!(uuid::Uuid::parse_str(roll["id"].as_str().unwrap()).is_ok());
    }
    t.alice.ok("roll", json!({ "kind": "coin" })).await;
    let roll = t.alice.expect("roll").await;
    assert_eq!(
        (&roll["kind"], &roll["player_name"]),
        (&json!("coin"), &json!("Alice"))
    );
    assert!(roll["result"] == "Heads" || roll["result"] == "Tails");

    for payload in [
        json!({ "kind": "dice", "sides": 1 }),
        json!({ "kind": "dice", "sides": 1001 }),
        json!({ "kind": "dice", "sides": 6.5 }),
        json!({ "kind": "dice", "sides": "20" }),
        json!({ "kind": "dice", "sides": 20, "result": 20 }),
        json!({ "kind": "coin", "player_name": "Bob" }),
        json!({ "kind": "coin", "result": "Heads" }),
        json!({ "kind": "other" }),
        json!({}),
    ] {
        t.alice
            .refused(
                "roll",
                payload,
                "invalid roll (dice must have 2–1000 sides)",
            )
            .await;
    }
    t.alice.refute("roll").await;
}

#[tokio::test]
async fn validates_elimination_in_own_status_without_changing_life() {
    let mut t = Table::new().await;
    t.alice
        .ok("update_status", json!({ "eliminated": true }))
        .await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (&meta["eliminated"], &meta["life"]),
        (&json!(true), &json!(40))
    );
    t.alice
        .err("update_status", json!({ "eliminated": "true" }))
        .await;
    t.alice
        .ok("update_status", json!({ "eliminated": false }))
        .await;
    t.wait_meta(PEER_A, |meta| meta["eliminated"] == false)
        .await;
}

#[tokio::test]
async fn reaching_zero_life_eliminates_the_seat_and_regaining_life_does_not_restore_it() {
    let mut t = Table::new().await;
    t.alice.ok("update_status", json!({ "life": 1 })).await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (&meta["eliminated"], &meta["life"]),
        (&json!(false), &json!(1))
    );
    t.alice.refute("eliminated_seats").await;

    t.alice.ok("update_status", json!({ "life": 0 })).await;
    let event = t.alice.expect("eliminated_seats").await;
    assert_eq!(event["participants"].as_array().unwrap().len(), 1);
    assert_eq!(
        (
            &event["participants"][0]["peer_id"],
            &event["participants"][0]["eliminated"]
        ),
        (&json!(PEER_A), &json!(true))
    );
    let meta = t.wait_meta(PEER_A, |meta| meta["life"] == 0).await;
    assert_eq!(meta["eliminated"], true);

    t.alice.ok("update_status", json!({ "life": 5 })).await;
    let meta = t.meta(PEER_A);
    assert_eq!(
        (&meta["eliminated"], &meta["life"]),
        (&json!(true), &json!(5))
    );

    // An explicit restore in the same update wins over the zero-life rule.
    t.alice
        .ok("update_status", json!({ "life": -3, "eliminated": false }))
        .await;
    t.alice
        .expect_where("eliminated_seats", |event| {
            event["participants"] == json!([])
        })
        .await;
    let meta = t.wait_meta(PEER_A, |meta| meta["life"] == -3).await;
    assert_eq!(meta["eliminated"], false);
}

#[tokio::test]
async fn only_the_owner_or_the_seat_itself_can_eliminate_and_restore_a_player() {
    let mut t = Table::new().await;
    let mut other = t.join_player(PEER_B, "Bob").await;
    other
        .err(
            "set_eliminated",
            json!({ "peer_id": PEER_A, "eliminated": true }),
        )
        .await;
    t.alice
        .ok(
            "set_eliminated",
            json!({ "peer_id": PEER_A, "eliminated": true }),
        )
        .await;
    let event = t.alice.expect("eliminated_seats").await;
    assert_eq!(event["participants"][0]["peer_id"], PEER_A);
    t.alice.ok("update_status", json!({ "life": 7 })).await;
    let meta = t.wait_meta(PEER_A, |meta| meta["life"] == 7).await;
    assert_eq!(meta["eliminated"], true);

    t.alice
        .ok(
            "set_eliminated",
            json!({ "peer_id": PEER_A, "eliminated": false }),
        )
        .await;
    t.alice
        .expect_where("eliminated_seats", |event| {
            event["participants"] == json!([])
        })
        .await;
    t.wait_meta(PEER_A, |meta| meta["eliminated"] == false)
        .await;

    for payload in [
        json!({}),
        json!({ "peer_id": PEER_A, "eliminated": 1 }),
        json!({ "peer_id": PEER_A, "eliminated": true, "life": 0 }),
    ] {
        other
            .refused("set_eliminated", payload, "invalid elimination")
            .await;
    }
    other
        .err(
            "set_eliminated",
            json!({ "peer_id": "ghost", "eliminated": true }),
        )
        .await;

    let mut foreign = t.server.join_player(&room_id(), ELSEWHERE, "Cara").await;
    foreign
        .err(
            "set_eliminated",
            json!({ "peer_id": PEER_A, "eliminated": true }),
        )
        .await;
}

#[tokio::test]
async fn departed_eliminated_seats_survive_and_rejoining_replaces_their_peer_id() {
    let mut t = Table::new().await;
    let mut other = t.join_player(PEER_B, "Bob").await;
    t.alice
        .ok("seat_order", json!({ "peer_ids": [PEER_A, PEER_B] }))
        .await;
    t.alice
        .ok("update_status", json!({ "eliminated": true }))
        .await;
    // A round trip lets the room's elimination notice update presence first.
    t.alice.ok("timer_sync", json!({})).await;
    let presence_ref = t.meta(PEER_A)["phx_ref"].clone();

    t.alice.leave().await;
    // Updates also list the replaced meta under `leaves`; the final leave carries the last ref.
    let diff = other
        .expect_where("presence_diff", |diff| {
            diff["leaves"][PEER_A]["metas"][0]["phx_ref"] == presence_ref
        })
        .await;
    assert!(diff["joins"].get(PEER_A).is_none());
    t.server.wait_departed(&t.room, t.player).await;

    other
        .err("seat_order", json!({ "peer_ids": [PEER_B] }))
        .await;
    let snapshot = t.snapshot().await;
    assert_eq!(snapshot.peer_ids, [PEER_A, PEER_B]);
    assert_eq!(
        snapshot
            .eliminated_seats
            .iter()
            .map(|seat| seat.player_id)
            .collect::<Vec<_>>(),
        [t.player]
    );

    let mut cara = t.join_player(PEER_C, "Cara").await;
    let state = cara.expect("table_state").await;
    assert_eq!(state["eliminated_seats"][0]["player_id"], t.player);
    assert_eq!(state["eliminated_seats"][0]["eliminated"], true);

    let mut rejoined = t.rejoin(PEER_A_NEW).await;
    let state = rejoined.expect("table_state").await;
    assert_eq!(state["peer_ids"], json!([PEER_A_NEW, PEER_B]));
    assert_eq!(state["eliminated_seats"][0]["peer_id"], PEER_A_NEW);
    rejoined
        .ok("update_status", json!({ "eliminated": false }))
        .await;
    assert!(t.snapshot().await.eliminated_seats.is_empty());
}

fn counts(pairs: &[(i64, i64)]) -> Value {
    Value::Object(
        pairs
            .iter()
            .map(|(id, count)| (id.to_string(), json!(count)))
            .collect(),
    )
}

fn turns_are(active: i64, expected: Value, revision: i64) -> impl Fn(&Value) -> bool {
    move |state| {
        state["turns"]["active_player_id"] == active
            && state["turns"]["counts"] == expected
            && state["turns"]["revision"] == revision
    }
}

#[tokio::test]
async fn shared_turns_start_in_order_pass_once_per_revision_and_survive_late_joins() {
    let mut t = Table::new().await;
    let mut other = t.join_player(PEER_B, "Bob").await;
    let bob = other.player_id();
    let alice = t.player;
    t.alice.err("pass_turn", json!({ "revision": 0 })).await;
    other
        .err("turn_settings", json!({ "auto_randomize": false }))
        .await;
    t.alice
        .ok("turn_settings", json!({ "auto_randomize": false }))
        .await;
    t.alice
        .expect_where("table_state", |state| state["auto_randomize"] == false)
        .await;
    t.alice.ok("start_game", json!({})).await;
    assert_eq!(
        t.alice.expect("seat_order").await,
        json!({ "peer_ids": [PEER_A, PEER_B], "shuffled": false })
    );
    t.alice
        .expect_where("table_state", turns_are(alice, counts(&[(alice, 1)]), 1))
        .await;

    let first = t.snapshot().await;
    assert_eq!(first.timer.paused_at, first.timer.started_at);
    other.ok("pass_turn", json!({ "revision": 1 })).await;
    t.alice.err("pass_turn", json!({ "revision": 1 })).await;
    // Passing the first turn before pressing Start still begins the clock.
    t.alice
        .expect_where("timer_state", |timer| timer["paused_at"].is_null())
        .await;
    t.alice
        .expect_where(
            "table_state",
            turns_are(bob, counts(&[(alice, 1), (bob, 1)]), 2),
        )
        .await;

    // Un-pass hands the turn back once per revision, and passing again restores it.
    t.alice.err("unpass_turn", json!({ "revision": 1 })).await;
    t.alice.err("unpass_turn", json!({})).await;
    t.alice.ok("unpass_turn", json!({ "revision": 2 })).await;
    t.alice
        .expect_where(
            "table_state",
            turns_are(alice, counts(&[(alice, 1), (bob, 0)]), 3),
        )
        .await;
    other.err("unpass_turn", json!({ "revision": 3 })).await;
    other.ok("pass_turn", json!({ "revision": 3 })).await;
    t.alice
        .expect_where(
            "table_state",
            turns_are(bob, counts(&[(alice, 1), (bob, 1)]), 4),
        )
        .await;

    t.alice
        .ok("adjust_turn", json!({ "player_id": alice, "delta": 1 }))
        .await;
    assert_eq!(t.snapshot().await.turns.counts[&alice], 2);
    let paused = t.alice.ok("timer", json!({ "action": "pause" })).await;
    t.alice.ok("pass_turn", json!({ "revision": 4 })).await;
    let current = t.snapshot().await;
    assert_eq!(current.turns.counts, [(alice, 3), (bob, 1)].into());
    assert_eq!(current.turns.active_player_id, Some(alice));
    let paused_timer: the_gathering::webcam::room::TimerState =
        serde_json::from_value(paused.clone()).unwrap();
    assert_eq!(
        current.turns.started_elapsed_ms,
        paused_timer.timer().elapsed(paused_timer.server_now)
    );
    assert_eq!(current.timer.started_at, first.timer.started_at);

    let mut cara = t.join_player(PEER_C, "Cara").await;
    let expected = serde_json::to_value(&current.turns).unwrap();
    let state = cara.expect("table_state").await;
    assert_eq!(
        (&state["turns"], &state["auto_randomize"]),
        (&expected, &json!(false))
    );
    // A reshuffle leaves the active player, counts and pause untouched.
    t.alice
        .err(
            "seat_order",
            json!({ "peer_ids": [PEER_C, PEER_B, PEER_A] }),
        )
        .await;
    t.alice
        .ok("seat_order", json!({ "peer_ids": [PEER_B, PEER_A] }))
        .await;
    let snapshot = t.snapshot().await;
    assert_eq!(snapshot.turns, current.turns);
    assert_eq!(snapshot.timer.paused_at, paused["paused_at"].as_i64());
}

#[tokio::test]
async fn eliminating_skips_turns_but_disconnecting_does_not_advance_the_game() {
    let mut t = Table::new().await;
    let mut other = t.join_player(PEER_B, "Bob").await;
    let bob = other.player_id();
    let alice = t.player;
    t.alice
        .ok("seat_order", json!({ "peer_ids": [PEER_A, PEER_B] }))
        .await;
    t.alice
        .ok("update_status", json!({ "eliminated": true }))
        .await;
    t.alice
        .expect_where("table_state", |state| {
            state["turns"]["active_player_id"] == bob && state["turns"]["revision"] == 2
        })
        .await;
    other.ok("pass_turn", json!({ "revision": 2 })).await;
    assert_eq!(
        t.snapshot().await.turns.counts,
        [(alice, 1), (bob, 2)].into()
    );
    t.alice
        .ok("update_status", json!({ "eliminated": false }))
        .await;
    t.server.disconnect(&mut other, &t.room).await;

    let turns = t.snapshot().await.turns;
    assert_eq!((turns.active_player_id, turns.revision), (Some(bob), 3));
    assert_eq!(turns.counts, [(alice, 1), (bob, 2)].into());
}

#[tokio::test]
async fn validates_turn_requests_and_rejects_forged_counts_times_and_unknown_players() {
    let mut t = Table::new().await;
    for (event, payload) in [
        ("start_game", json!({ "started_at": 1 })),
        ("turn_settings", json!({ "auto_randomize": "false" })),
        (
            "turn_settings",
            json!({ "auto_randomize": true, "order": [] }),
        ),
        ("pass_turn", json!({})),
        ("pass_turn", json!({ "revision": -1 })),
        ("pass_turn", json!({ "revision": 1.5 })),
        ("pass_turn", json!({ "revision": 0, "elapsed_ms": 0 })),
        ("adjust_turn", json!({ "player_id": -1, "delta": 1 })),
        ("adjust_turn", json!({ "player_id": 1, "delta": 2 })),
        (
            "adjust_turn",
            json!({ "player_id": 1, "delta": -1, "count": 999 }),
        ),
        ("adjust_turn", json!({ "player_id": "1", "delta": 1 })),
    ] {
        t.alice.err(event, payload).await;
    }
}

#[allow(clippy::needless_pass_by_value)] // Call sites pass `json!` literals.
fn card_entry(owner: &str, overrides: Value) -> Value {
    let mut entry = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "ownerPeerId": owner,
        "at": 123,
        "card": { "id": "art-1", "name": "Forest", "set": "lea" },
    });
    for (key, value) in overrides.as_object().unwrap() {
        entry[key] = value.clone();
    }
    entry
}

#[tokio::test]
async fn card_attribution_is_stamped_from_the_senders_seat_never_the_payload() {
    let mut t = Table::new().await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    let spoofed = card_entry(PEER_A, json!({ "byPlayerName": "Alice" }));
    bob.ok(
        "cards",
        json!({ "type": "card_identified", "entry": spoofed }),
    )
    .await;
    let event = t.alice.expect("identified_cards").await;
    assert_eq!(event["entries"][0]["byPlayerName"], "Bob");
    assert_eq!(event["type"], "card_identified");
    assert_eq!(
        event["by"],
        json!({ "peer_id": PEER_B, "player_name": "Bob" })
    );
    assert_eq!(t.snapshot().await.cards[0]["byPlayerName"], "Bob");

    // The name is optional on the wire.
    let unnamed = card_entry(
        PEER_B,
        json!({ "card": { "id": "a", "name": "Island", "set": "x" } }),
    );
    t.alice
        .ok(
            "cards",
            json!({ "type": "card_identified", "entry": unnamed }),
        )
        .await;
    let names: Vec<Value> = t
        .snapshot()
        .await
        .cards
        .iter()
        .map(|card| card["byPlayerName"].clone())
        .collect();
    assert_eq!(names, [json!("Bob"), json!("Alice")]);
}

#[tokio::test]
async fn any_seat_may_remove_an_entry_and_the_broadcast_names_the_remover() {
    let mut t = Table::new().await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    let entry = card_entry(PEER_A, json!({}));
    t.alice
        .ok(
            "cards",
            json!({ "type": "card_identified", "entry": entry }),
        )
        .await;
    bob.ok(
        "cards",
        json!({ "type": "card_removed", "id": entry["id"] }),
    )
    .await;
    let event = t
        .alice
        .expect_where("identified_cards", |event| event["type"] == "card_removed")
        .await;
    assert_eq!(event["entries"], json!([]));
    assert_eq!(
        event["by"],
        json!({ "peer_id": PEER_B, "player_name": "Bob" })
    );
    assert!(t.snapshot().await.cards.is_empty());
}

#[tokio::test]
async fn only_the_board_owner_can_clear_its_cards() {
    let mut t = Table::new().await;
    let mut bob = t.join_player(PEER_B, "Bob").await;
    let entry = card_entry(PEER_A, json!({}));
    t.alice
        .ok(
            "cards",
            json!({ "type": "card_identified", "entry": entry }),
        )
        .await;
    bob.refused(
        "cards",
        json!({ "type": "cards_cleared", "ownerPeerId": PEER_A }),
        "only the board owner can clear its cards",
    )
    .await;
    assert_eq!(t.snapshot().await.cards.len(), 1);
    t.alice
        .ok(
            "cards",
            json!({ "type": "cards_cleared", "ownerPeerId": PEER_A }),
        )
        .await;
    assert!(t.snapshot().await.cards.is_empty());
    t.alice
        .refused("cards", json!({ "type": "card_flipped" }), "invalid cards")
        .await;
}

#[tokio::test]
async fn spectators_cannot_identify_remove_or_clear_cards() {
    let mut t = Table::new().await;
    let entry = card_entry(PEER_A, json!({}));
    // Starting the game makes later arrivals spectators (and clears lobby cards).
    t.alice
        .ok("seat_order", json!({ "peer_ids": [PEER_A] }))
        .await;
    t.alice
        .ok(
            "cards",
            json!({ "type": "card_identified", "entry": entry }),
        )
        .await;
    let mut spectator = t.join_player(PEER_B, "Bob").await;
    assert_eq!(spectator.participant["spectator"], true);
    for payload in [
        json!({ "type": "card_identified", "entry": card_entry(PEER_A, json!({})) }),
        json!({ "type": "card_removed", "id": entry["id"] }),
        json!({ "type": "cards_cleared", "ownerPeerId": PEER_B }),
    ] {
        spectator
            .refused("cards", payload, "spectators cannot change the game")
            .await;
    }
    let cards = t.snapshot().await.cards;
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["id"], entry["id"]);
}
