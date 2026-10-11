//! Webcam table game modes: owner/admin controls, Two-Headed Giant and start-of-game seat
//! randomization.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::webcam_support;

use serde_json::{Value, json};
use the_gathering::webcam::{Mode, session};
use webcam_support::{Client, PEER_A, PEER_B, PEER_C, PEER_D, PEER_E, PEER_F, Table};

const MATE_RETURNED: &str = "00000000-0000-4000-8000-00000000b0b2";
const SPECTATOR_PEER: &str = "00000000-0000-4000-8000-000000005bec";

#[tokio::test]
async fn mode_is_owner_only_validates_roster_and_freezes_order_after_start() {
    let mut t = Table::new().await;
    let mut other = t.join_seat(PEER_B).await;
    other.err("set_mode", json!({ "mode": "five_star" })).await;
    t.alice
        .refused(
            "set_mode",
            json!({ "mode": "invalid" }),
            "invalid game mode",
        )
        .await;
    t.alice.ok("set_mode", json!({ "mode": "five_star" })).await;
    t.alice
        .refused(
            "start_game",
            json!({}),
            "Five Star requires exactly 5 players",
        )
        .await;

    let mut seats = vec![t.join_seat(PEER_C).await, t.join_seat(PEER_D).await];
    t.alice.err("start_game", json!({})).await;
    seats.push(t.join_seat(PEER_E).await);
    let peers = [PEER_E, PEER_A, PEER_C, PEER_B, PEER_D];
    other
        .err("arrange_seats", json!({ "peer_ids": peers }))
        .await;
    t.alice
        .ok("arrange_seats", json!({ "peer_ids": peers }))
        .await;
    assert_eq!(t.snapshot().await.timer.started_at, None);
    t.alice
        .ok("turn_settings", json!({ "auto_randomize": false }))
        .await;
    t.alice.ok("start_game", json!({})).await;
    assert_eq!(t.snapshot().await.peer_ids, peers);
    t.alice
        .refused(
            "set_mode",
            json!({ "mode": "commander" }),
            "game mode is fixed after start",
        )
        .await;
    let reversed: Vec<&str> = peers.iter().rev().copied().collect();
    t.alice
        .refused(
            "seat_order",
            json!({ "peer_ids": reversed }),
            "seat order is fixed after start",
        )
        .await;
    t.alice
        .err("arrange_seats", json!({ "peer_ids": reversed }))
        .await;
    assert_eq!(t.snapshot().await.peer_ids, peers);
    let mut spectator = t.join_seat(SPECTATOR_PEER).await;
    spectator
        .err("set_mode", json!({ "mode": "commander" }))
        .await;
}

async fn join_admin(t: &Table, peer: &str) -> Client {
    let user = t.server.admin().await;
    let player = t
        .server
        .player(&format!("Admin {peer}"), Some(user.id))
        .await;
    t.server.join_as(&user, player, &t.room, peer).await
}

#[tokio::test]
async fn hosts_remove_players_and_spectating_admins_remove_spectators() {
    let mut t = Table::new().await;
    let (user, player) = t.server.linked_player("Removed player").await;
    let mut target = t.server.join_as(&user, player, &t.room, PEER_B).await;
    target
        .refused(
            "remove_participant",
            json!({"peer_id": PEER_A}),
            "only the host or an admin can remove participants",
        )
        .await;
    t.alice
        .err("remove_participant", json!({"peer_id": PEER_A}))
        .await;
    t.alice
        .err("remove_participant", json!({"player_id": player}))
        .await;
    t.alice
        .err("remove_participant", json!({"peer_id": PEER_F}))
        .await;
    t.alice
        .ok("remove_participant", json!({"peer_id": PEER_B}))
        .await;
    target.expect("removed").await;
    assert!(
        !t.snapshot()
            .await
            .seats
            .iter()
            .any(|seat| seat.player_id == player)
    );
    webcam_support::wait_until(|| async {
        !t.server
            .state()
            .presence
            .has_key(&the_gathering::webcam::topic(&t.room), PEER_B)
    })
    .await;
    let (status, response, _) = t.server.try_join(&user, player, &t.room, PEER_C).await;
    assert_eq!(status, "error");
    assert_eq!(response["reason"], "You were removed from this table.");
    let saved = session::load(t.server.app.pool(), &t.room)
        .await
        .unwrap()
        .unwrap();
    assert!(saved.removed_players.contains(&player));

    t.alice.ok("start_game", json!({})).await;
    let mut spectator = t.join_seat(PEER_D).await;
    assert_eq!(spectator.participant["spectator"], true);
    spectator
        .err("remove_participant", json!({"peer_id": PEER_A}))
        .await;
    let mut admin = join_admin(&t, PEER_E).await;
    assert!(
        !admin.owner,
        "a spectating admin must not gain game controls"
    );
    admin
        .ok("remove_participant", json!({"peer_id": PEER_D}))
        .await;
    spectator.expect("removed").await;
    admin.err("start_game", json!({})).await;
    t.alice.ok("rematch", json!({})).await;
    let (status, _, _) = t.server.try_join(&user, player, &t.room, PEER_F).await;
    assert_eq!(status, "error", "rematching must not undo removal");
    t.server.state().webcam_tables.kill(&t.room);
    t.alice.expect("rejoin").await;
    let (status, response, _) = t.server.try_join(&user, player, &t.room, PEER_F).await;
    assert_eq!(status, "error");
    assert_eq!(response["reason"], "You were removed from this table.");
    let other_room = webcam_support::room_id();
    let (status, _, _) = t.server.try_join(&user, player, &other_room, PEER_F).await;
    assert_eq!(status, "ok", "removal is table-scoped, not an account ban");
}

#[tokio::test]
async fn removing_the_active_player_keeps_their_result_and_advances_the_turn() {
    let mut t = Table::new().await;
    let mut bob = t.join_seat(PEER_B).await;
    let _carol = t.join_seat(PEER_C).await;
    t.alice
        .ok(
            "arrange_seats",
            json!({"peer_ids": [PEER_B, PEER_C, PEER_A]}),
        )
        .await;
    t.alice.ok("start_game", json!({"randomize": false})).await;
    assert_eq!(
        t.snapshot().await.turns.active_player_id,
        Some(bob.player_id())
    );
    t.alice
        .ok("remove_participant", json!({"peer_id": PEER_B}))
        .await;
    bob.expect("removed").await;
    let snapshot = t.snapshot().await;
    assert_eq!(snapshot.seats.len(), 3);
    assert!(
        snapshot
            .eliminated_seats
            .iter()
            .any(|seat| seat.player_id == bob.player_id())
    );
    let carol = snapshot
        .seats
        .iter()
        .find(|seat| seat.peer_id == PEER_C)
        .unwrap();
    assert_eq!(snapshot.turns.active_player_id, Some(carol.player_id));
    assert_eq!(snapshot.peer_ids, [PEER_B, PEER_C, PEER_A]);
}

#[tokio::test]
async fn removing_a_teammate_preserves_teams_and_cannot_be_undone_by_restoring_the_team() {
    let mut t = Table::new().await;
    let mut bob = t.join_seat(PEER_B).await;
    let _carol = t.join_seat(PEER_C).await;
    let _dave = t.join_seat(PEER_D).await;
    t.alice
        .ok("set_mode", json!({"mode": "two_headed_giant"}))
        .await;
    t.alice
        .ok(
            "arrange_seats",
            json!({"peer_ids": [PEER_A, PEER_B, PEER_C, PEER_D]}),
        )
        .await;
    t.alice.ok("start_game", json!({"randomize": false})).await;
    t.alice
        .ok("remove_participant", json!({"peer_id": PEER_B}))
        .await;
    bob.expect("removed").await;
    let snapshot = t.snapshot().await;
    assert_eq!(snapshot.eliminated_seats.len(), 1);
    assert_eq!(snapshot.eliminated_seats[0].player_id, bob.player_id());
    t.alice
        .ok(
            "set_eliminated",
            json!({"peer_id": PEER_A, "eliminated": true}),
        )
        .await;
    t.alice
        .ok(
            "set_eliminated",
            json!({"peer_id": PEER_A, "eliminated": false}),
        )
        .await;
    let restored = t.snapshot().await;
    assert_eq!(restored.peer_ids, snapshot.peer_ids);
    assert_eq!(restored.team_life, snapshot.team_life);
    assert_eq!(restored.eliminated_seats.len(), 1);
    assert_eq!(restored.eliminated_seats[0].player_id, bob.player_id());
}

#[tokio::test]
async fn admins_hold_table_controls_in_rooms_they_did_not_open() {
    let t = Table::new().await;
    let mut admin = join_admin(&t, PEER_B).await;
    assert!(admin.owner);
    let mut member = t.join_seat(PEER_C).await;
    let _dave = t.join_seat(PEER_D).await;

    member
        .err("set_mode", json!({ "mode": "two_headed_giant" }))
        .await;
    admin
        .ok("set_mode", json!({ "mode": "two_headed_giant" }))
        .await;

    // Teams are adjacent pairs: Alice and C, then the admin and D.
    admin
        .ok(
            "arrange_seats",
            json!({ "peer_ids": [PEER_A, PEER_C, PEER_B, PEER_D] }),
        )
        .await;
    admin
        .ok("turn_settings", json!({ "auto_randomize": false }))
        .await;
    admin.ok("start_game", json!({})).await;

    member
        .err("adjust_team_life", json!({ "team_index": 1, "delta": -1 }))
        .await;
    admin
        .ok("adjust_team_life", json!({ "team_index": 0, "delta": -1 }))
        .await;
    assert_eq!(t.snapshot().await.team_life[&0], 59);
    admin
        .ok(
            "set_eliminated",
            json!({ "peer_id": PEER_C, "eliminated": true }),
        )
        .await;

    // A late admin spectates, and spectators never hold table controls.
    let mut spectator = join_admin(&t, SPECTATOR_PEER).await;
    assert!(!spectator.owner);
    spectator.err("timer", json!({ "action": "pause" })).await;
}

fn eliminated_peers(seats: &[the_gathering::webcam::seat::Seat]) -> Vec<String> {
    let mut peers: Vec<String> = seats.iter().map(|seat| seat.peer_id.clone()).collect();
    peers.sort();
    peers
}

#[tokio::test]
async fn two_headed_giant_validates_teams_serializes_shared_life_and_eliminates_offline_teammates()
{
    let mut t = Table::new().await;
    let mut mate = t.join_seat(PEER_B).await;
    t.alice
        .ok("set_mode", json!({ "mode": "two_headed_giant" }))
        .await;
    t.alice.err("start_game", json!({})).await;
    let mut rival = t.join_seat(PEER_C).await;
    t.alice
        .refused(
            "start_game",
            json!({}),
            "Two-Headed Giant requires an even number of players (at least 4)",
        )
        .await;
    let _dave = t.join_seat(PEER_D).await;

    t.alice
        .ok(
            "arrange_seats",
            json!({ "peer_ids": [PEER_A, PEER_B, PEER_C, PEER_D] }),
        )
        .await;
    t.alice
        .ok("turn_settings", json!({ "auto_randomize": false }))
        .await;
    t.alice
        .err("adjust_team_life", json!({ "team_index": 0, "delta": 1 }))
        .await;
    t.alice.ok("start_game", json!({})).await;
    assert_eq!(t.snapshot().await.team_life, [(0, 60), (1, 60)].into());
    mate.ok("adjust_team_life", json!({ "team_index": 0, "delta": -7 }))
        .await;
    t.alice
        .ok("adjust_team_life", json!({ "team_index": 0, "delta": 2 }))
        .await;
    assert_eq!(t.snapshot().await.team_life[&0], 55);
    rival
        .refused(
            "adjust_team_life",
            json!({ "team_index": 0, "delta": -1 }),
            "only teammates or the owner can change a started team's life",
        )
        .await;
    t.alice
        .ok(
            "adjust_team_life",
            json!({ "team_index": 1, "delta": 1998 }),
        )
        .await;
    assert_eq!(t.snapshot().await.team_life[&1], 999);
    t.alice
        .ok(
            "adjust_team_life",
            json!({ "team_index": 1, "delta": -1998 }),
        )
        .await;
    assert_eq!(t.snapshot().await.team_life[&1], -999);

    // Zero shared life knocks out the whole team, but only that team.
    let event = t.alice.expect("eliminated_seats").await;
    let mut knocked_out: Vec<Value> = event["participants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seat| seat["peer_id"].clone())
        .collect();
    knocked_out.sort_by_key(ToString::to_string);
    assert_eq!(knocked_out, [json!(PEER_C), json!(PEER_D)]);
    assert!(
        t.snapshot()
            .await
            .seats
            .iter()
            .all(|seat| seat.eliminated == [PEER_C, PEER_D].contains(&seat.peer_id.as_str()))
    );

    // Gaining life back does not restore; the owner restores the team explicitly.
    t.alice
        .ok(
            "adjust_team_life",
            json!({ "team_index": 1, "delta": 1000 }),
        )
        .await;
    assert_eq!(t.snapshot().await.eliminated_seats.len(), 2);
    t.alice
        .ok(
            "set_eliminated",
            json!({ "peer_id": PEER_C, "eliminated": false }),
        )
        .await;
    assert!(t.snapshot().await.eliminated_seats.is_empty());
    t.alice
        .ok(
            "adjust_team_life",
            json!({ "team_index": 1, "delta": -1000 }),
        )
        .await;
    assert_eq!(t.snapshot().await.team_life[&1], -999);
    t.alice
        .ok(
            "set_eliminated",
            json!({ "peer_id": PEER_D, "eliminated": false }),
        )
        .await;
    assert!(t.snapshot().await.eliminated_seats.is_empty());

    for payload in [
        json!({ "team_index": -1, "delta": 1 }),
        json!({ "team_index": 9, "delta": 1 }),
        json!({ "team_index": 0, "delta": 1.5 }),
    ] {
        t.alice.err("adjust_team_life", payload).await;
    }

    t.alice
        .ok(
            "adjust_turn",
            json!({ "player_id": mate.player_id(), "delta": 1 }),
        )
        .await;
    assert_eq!(t.snapshot().await.turns.counts, [(t.player, 2)].into());
    mate.ok("pass_turn", json!({ "revision": 1 })).await;
    assert_eq!(
        t.snapshot().await.turns.active_player_id,
        Some(rival.player_id())
    );

    let mate_player = mate.player_id();
    t.server.disconnect(&mut mate, &t.room).await;
    t.alice
        .ok(
            "set_eliminated",
            json!({ "peer_id": PEER_A, "eliminated": true }),
        )
        .await;
    assert_eq!(
        eliminated_peers(&t.snapshot().await.eliminated_seats),
        [PEER_A, PEER_B]
    );

    t.alice.ok("update_status", json!({ "life": 25 })).await;
    let mate_user = t
        .server
        .state()
        .accounts
        .get_user(mate_user_id(&t, mate_player).await)
        .await
        .unwrap()
        .unwrap();
    let mut restored = t
        .server
        .rejoin(&t.room, &mate_user, mate_player, MATE_RETURNED)
        .await;
    assert_eq!(restored.participant["eliminated"], true);
    restored
        .ok("update_status", json!({ "eliminated": false }))
        .await;
    assert!(t.snapshot().await.eliminated_seats.is_empty());
    t.alice.ok("start_game", json!({})).await;
    assert_eq!(t.snapshot().await.team_life, [(0, 55), (1, -999)].into());
    let saved = session::load(t.server.app.pool(), &t.room)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.mode, Mode::TwoHeadedGiant);
    assert_eq!(saved.team_life, [(0, 55), (1, -999)].into());
    assert_eq!(saved.turns, t.snapshot().await.turns);
}

async fn mate_user_id(t: &Table, player: i64) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT user_id FROM players WHERE id = ?")
        .bind(player)
        .fetch_one(t.server.app.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn two_headed_giant_randomizes_whole_pairs_and_older_snapshot_formats_start_fresh() {
    let mut t = Table::new().await;
    let mut seats = Vec::new();
    for peer in [PEER_B, PEER_C, PEER_D, PEER_E] {
        seats.push(t.join_seat(peer).await);
    }
    t.alice
        .ok("set_mode", json!({ "mode": "two_headed_giant" }))
        .await;
    t.alice.err("start_game", json!({})).await;
    seats.push(t.join_seat(PEER_F).await);
    let peers = [PEER_D, PEER_A, PEER_F, PEER_B, PEER_E, PEER_C];
    t.alice
        .ok("arrange_seats", json!({ "peer_ids": peers }))
        .await;
    t.alice.ok("start_game", json!({})).await;

    let pairs = |ids: &[String]| {
        let mut pairs: Vec<Vec<String>> = ids.chunks(2).map(<[String]>::to_vec).collect();
        pairs.sort();
        pairs
    };
    let arranged: Vec<String> = peers.iter().map(ToString::to_string).collect();
    assert_eq!(pairs(&t.snapshot().await.peer_ids), pairs(&arranged));

    let pool = t.server.app.pool();
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT snapshot FROM webcam_table_sessions WHERE id = ?")
            .bind(&t.room)
            .fetch_one(pool)
            .await
            .unwrap();
    let mut legacy: Value = serde_json::from_slice(&stored).unwrap();
    legacy["version"] = json!(1);
    sqlx::query("UPDATE webcam_table_sessions SET snapshot = ? WHERE id = ?")
        .bind(serde_json::to_vec(&legacy).unwrap())
        .bind(&t.room)
        .execute(pool)
        .await
        .unwrap();
    assert!(session::load(pool, &t.room).await.unwrap().is_none());

    // An Erlang term (or anything else that is not this JSON) is treated as missing too.
    sqlx::query("UPDATE webcam_table_sessions SET snapshot = ? WHERE id = ?")
        .bind(vec![131u8, 116, 0, 0, 0, 0])
        .bind(&t.room)
        .execute(pool)
        .await
        .unwrap();
    assert!(session::load(pool, &t.room).await.unwrap().is_none());
}

#[tokio::test]
async fn start_game_with_randomize_false_keeps_the_arranged_order_despite_auto_randomize() {
    let mut t = Table::new().await;
    let mut seats = Vec::new();
    for peer in [PEER_B, PEER_C, PEER_D] {
        seats.push(t.join_seat(peer).await);
    }
    let peers = [PEER_D, PEER_A, PEER_B, PEER_C];
    t.alice
        .ok("arrange_seats", json!({ "peer_ids": peers }))
        .await;
    t.alice
        .refused(
            "start_game",
            json!({ "randomize": "false" }),
            "invalid start",
        )
        .await;
    t.alice
        .err("start_game", json!({ "randomize": false, "extra": 1 }))
        .await;
    assert_eq!(t.snapshot().await.timer.started_at, None);
    assert!(t.snapshot().await.auto_randomize);

    t.alice
        .ok("start_game", json!({ "randomize": false }))
        .await;
    assert_eq!(
        t.alice.expect("seat_order").await,
        json!({ "peer_ids": peers, "shuffled": false })
    );
    assert_eq!(t.snapshot().await.peer_ids, peers);
    assert!(t.snapshot().await.timer.started_at.is_some());
}

#[tokio::test]
async fn start_game_with_randomize_true_shuffles_even_when_auto_randomize_is_off() {
    let mut t = Table::new().await;
    let mut seats = Vec::new();
    for peer in [PEER_B, PEER_C, PEER_D] {
        seats.push(t.join_seat(peer).await);
    }
    let peers = [PEER_D, PEER_A, PEER_B, PEER_C];
    t.alice
        .ok("arrange_seats", json!({ "peer_ids": peers }))
        .await;
    t.alice
        .ok("turn_settings", json!({ "auto_randomize": false }))
        .await;
    t.alice.ok("start_game", json!({ "randomize": true })).await;
    let order = t.alice.expect("seat_order").await;
    assert_eq!(order["shuffled"], true);
    let shuffled: Vec<String> = serde_json::from_value(order["peer_ids"].clone()).unwrap();
    let mut sorted = shuffled.clone();
    sorted.sort();
    let mut expected: Vec<String> = peers.iter().map(ToString::to_string).collect();
    expected.sort();
    assert_eq!(sorted, expected);
    assert_eq!(t.snapshot().await.peer_ids, shuffled);
}
