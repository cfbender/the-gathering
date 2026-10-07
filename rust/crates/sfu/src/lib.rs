//! The webcam table SFU (`TheGathering.WebcamTables.Sfu` and `Sfu.Room`).
//!
//! Every seat (and spectator) holds one WebRTC connection to this server. Each seat
//! publishes its camera as simulcast layers `l`, `m`, and `h`; the server forwards one
//! layer of every other seat to each connection, picking the layer each viewer asks for.
//! Signaling travels over the seat's Phoenix channel: the channel calls the methods here,
//! and the SFU sends [`SfuEvent`]s back on the channel's event sender.
//!
//! This file defines the interface the webcam table channel uses. The media
//! implementation replaces the bodies.

use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::mpsc;

pub mod browser_sdp;
mod codec;
pub mod ice_report;
mod ids;
mod layer;
mod munger;
pub mod simulcast_sdp;
mod subscription;

pub use layer::Layer;

/// Simulcast layer names, lowest resolution first; the browser encodes them in this order.
pub const LAYERS: [&str; 3] = ["l", "m", "h"];

/// Whether `layer` names a simulcast layer.
pub fn valid_layer(layer: &str) -> bool {
    LAYERS.contains(&layer)
}

/// A TURN/STUN server for relay-only operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IceServer {
    /// `turn:`/`turns:`/`stun:` URLs.
    pub urls: Vec<String>,
    /// Username, for TURN.
    pub username: Option<String>,
    /// Credential, for TURN.
    pub credential: Option<String>,
}

/// Fetches short-lived relay credentials (Cloudflare TURN) when a connection starts.
pub type RelayServers =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Vec<IceServer>> + Send>> + Send + Sync>;

/// Transport settings (`config :the_gathering, :sfu`).
#[derive(Clone)]
pub struct Settings {
    /// First UDP port for media.
    pub port_min: u16,
    /// Last UDP port for media.
    pub port_max: u16,
    /// The address announced to browsers instead of the host address.
    pub public_ip: Option<IpAddr>,
    /// Also gather IPv6 host candidates.
    pub ipv6: bool,
    /// Relay every connection through these TURN servers (relay-only mode), when set.
    pub relay: Option<RelayServers>,
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("port_min", &self.port_min)
            .field("port_max", &self.port_max)
            .field("public_ip", &self.public_ip)
            .field("ipv6", &self.ipv6)
            .field("relay", &self.relay.is_some())
            .finish()
    }
}

/// Server-initiated signaling delivered to a seat's channel, pushed to the browser as the
/// event of the same name (`{:sfu, :offer | :candidate | :peer_message | :down, payload}`).
#[derive(Clone, Debug, PartialEq)]
pub enum SfuEvent {
    /// Push `sfu_offer` with this payload (the set of boards changed).
    Offer(Value),
    /// Push `sfu_candidate` with this payload (trickle ICE).
    Candidate(Value),
    /// Push `peer_message` with this payload (another seat addressed this one).
    PeerMessage(Value),
    /// The media connection failed or crashed; the channel stops with an error so the
    /// client rejoins under a new peer id.
    Down(String),
}

/// Why an SFU call failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SfuError {
    /// The peer has no connection in this room (`{:error, :not_joined}`).
    #[error("not joined")]
    NotJoined,
    /// The browser sent something the connection could not apply.
    #[error("{0}")]
    Rejected(String),
    /// The SFU could not start a connection.
    #[error("sfu unavailable: {0}")]
    Unavailable(String),
}

/// Rooms of peer connections, keyed by table id.
#[derive(Debug)]
pub struct Sfu {
    settings: Settings,
}

impl Sfu {
    /// An SFU with no rooms yet.
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    /// What the browser needs to know: whether media reaches the server directly or via TURN.
    pub fn client_info(&self) -> Value {
        json!({ "transport": if self.settings.relay.is_some() { "relay" } else { "direct" } })
    }

    /// Registers `peer_id` at `room_id`; events for it go to `events`. When the sender's
    /// receiver is dropped the SFU treats the seat as gone, as if [`Sfu::leave`] were called.
    pub async fn join(
        &self,
        room_id: &str,
        peer_id: &str,
        spectator: bool,
        events: mpsc::UnboundedSender<SfuEvent>,
    ) -> Result<(), SfuError> {
        let _ = (room_id, peer_id, spectator, events);
        Ok(())
    }

    /// The seat's channel ended.
    pub async fn leave(&self, room_id: &str, peer_id: &str) {
        let _ = (room_id, peer_id);
    }

    /// Applies the browser's initial offer and returns the answer SDP.
    pub async fn offer(&self, room_id: &str, peer_id: &str, sdp: &str) -> Result<String, SfuError> {
        let _ = (room_id, peer_id, sdp);
        Err(SfuError::NotJoined)
    }

    /// Applies the browser's answer to a server offer.
    pub async fn answer(&self, room_id: &str, peer_id: &str, sdp: &str) -> Result<(), SfuError> {
        let _ = (room_id, peer_id, sdp);
        Err(SfuError::NotJoined)
    }

    /// Adds an ICE candidate (its JSON map) from the browser.
    pub async fn candidate(
        &self,
        room_id: &str,
        peer_id: &str,
        candidate: &Value,
    ) -> Result<(), SfuError> {
        let _ = (room_id, peer_id, candidate);
        Err(SfuError::NotJoined)
    }

    /// Asks for `layer` of `owner_id`'s video, from how large `peer_id` draws it.
    pub async fn layer(
        &self,
        room_id: &str,
        peer_id: &str,
        owner_id: &str,
        layer: &str,
    ) -> Result<(), SfuError> {
        let _ = (room_id, peer_id, owner_id, layer);
        Err(SfuError::NotJoined)
    }

    /// Limits `peer_id`'s video to `target` (another peer id), or to everyone with `None`.
    pub async fn reveal(
        &self,
        room_id: &str,
        peer_id: &str,
        target: Option<&str>,
    ) -> Result<(), SfuError> {
        let _ = (room_id, peer_id, target);
        Err(SfuError::NotJoined)
    }

    /// Delivers `message` to `to`'s channel as [`SfuEvent::PeerMessage`].
    pub async fn relay(
        &self,
        room_id: &str,
        from: &str,
        to: &str,
        message: Value,
    ) -> Result<(), SfuError> {
        let _ = (room_id, from, to, message);
        Err(SfuError::NotJoined)
    }
}
