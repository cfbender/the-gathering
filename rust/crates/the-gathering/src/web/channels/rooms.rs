//! Which webcam tables are open right now.
//!
//! Every running room is listed, including empty ones, until the pruner closes it after 30
//! idle minutes. Presence on the `webcam_tables` lobby topic supplies each room's connected
//! seats; spectators do not appear.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Value, json};

use crate::state::AppState;
use crate::webcam::seat::Seat;

/// The lobby presence topic.
pub const LOBBY_TOPIC: &str = "webcam_tables";
const MAX_PLAYERS: usize = 10;

/// A seat as the lobby lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LobbyPlayer {
    /// Player id.
    pub id: i64,
    /// Player name.
    pub name: String,
}

/// An open room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActiveRoom {
    /// Room id.
    pub id: String,
    /// When the room opened (ms).
    pub started_at: i64,
    /// Ten connected seats.
    pub full: bool,
    /// Connected seated players in join order.
    pub players: Vec<LobbyPlayer>,
}

/// Registers `owner`'s seat in the lobby.
pub fn track_seat(state: &AppState, owner: u64, room_id: &str, participant: &Seat) {
    state.presence.track(
        LOBBY_TOPIC,
        owner,
        &participant.peer_id,
        &json!({
            "room_id": room_id,
            "player_id": participant.player_id,
            "player_name": participant.player_name,
            "joined_at": participant.joined_at,
        }),
    );
}

/// Open rooms, oldest first, each with its connected seated players in join order.
pub fn active_rooms(state: &AppState) -> Vec<ActiveRoom> {
    let mut by_room: HashMap<String, Vec<(i64, i64, String)>> = HashMap::new();
    for meta in state.presence.metas(LOBBY_TOPIC) {
        let (Some(room), Some(player_id), Some(name)) = (
            meta.get("room_id").and_then(Value::as_str),
            meta.get("player_id").and_then(Value::as_i64),
            meta.get("player_name").and_then(Value::as_str),
        ) else {
            continue;
        };
        let joined_at = meta.get("joined_at").and_then(Value::as_i64).unwrap_or(0);
        by_room
            .entry(room.to_owned())
            .or_default()
            .push((joined_at, player_id, name.to_owned()));
    }
    let mut rooms: Vec<ActiveRoom> = state
        .webcam_tables
        .rooms()
        .into_iter()
        .map(|room| {
            let mut seats = by_room.remove(&room.id).unwrap_or_default();
            let mut seen = std::collections::HashSet::new();
            seats.retain(|(_, player_id, _)| seen.insert(*player_id));
            seats.sort_by_key(|(joined_at, _, _)| *joined_at);
            ActiveRoom {
                id: room.id,
                started_at: room.opened_at,
                full: seats.len() >= MAX_PLAYERS,
                players: seats
                    .into_iter()
                    .map(|(_, id, name)| LobbyPlayer { id, name })
                    .collect(),
            }
        })
        .collect();
    rooms.sort_by_key(|room| room.started_at);
    rooms
}
