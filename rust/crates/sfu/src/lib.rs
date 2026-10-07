//! The webcam table SFU.
//!
//! Every seat (and spectator) holds one WebRTC connection to this server. Each seat
//! publishes its camera as simulcast layers `l`, `m`, and `h`; the server forwards one
//! layer of every other seat to each connection, picking the layer each viewer asks for.
//! Signaling travels over the seat's Phoenix channel: the channel calls the methods here,
//! and the SFU sends [`SfuEvent`]s back on the channel's event sender.
//!
//! This file defines the interface the webcam table channel uses; [`room`] holds the media
//! side. Built on [str0m](https://github.com/algesten/str0m) in RTP mode: every room is one
//! task that owns its seats' connections and its UDP sockets, so forwarding a packet from a
//! publisher to its viewers is a function call.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use str0m::Candidate;
use tokio::sync::{mpsc, oneshot};

mod browser_sdp;
mod codec;
mod ice_report;
mod ids;
mod layer;
mod munger;
mod net;
mod room;
mod simulcast_sdp;
mod stun;
mod subscription;
mod turn;

pub use layer::Layer;

// Lets the shared test client (`tests/common`) name this crate from inside it.
#[cfg(test)]
extern crate self as the_gathering_sfu;
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod common;
#[cfg(test)]
mod relay_tests;

use ids::{PeerId, RoomId};
use room::{Command, Registry, Room, RoomEntry, Transport};
use turn::TurnServer;

/// Datagrams a room may have waiting before its sockets drop more.
const DATAGRAM_BACKLOG: usize = 4_096;

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
pub struct Sfu {
    settings: Settings,
    /// Interface addresses rooms bind their media sockets on.
    hosts: Vec<IpAddr>,
    rooms: Arc<Registry>,
    next_instance: AtomicU64,
}

impl std::fmt::Debug for Sfu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rooms = self.rooms.lock().map_or(0, |rooms| rooms.len());
        f.debug_struct("Sfu")
            .field("settings", &self.settings)
            .field("hosts", &self.hosts)
            .field("rooms", &rooms)
            .finish_non_exhaustive()
    }
}

impl Sfu {
    /// An SFU with no rooms yet. Media sockets bind on every non-loopback interface address
    /// (IPv4 only unless `settings.ipv6`), or on loopback when there is no other.
    pub fn new(settings: Settings) -> Self {
        let hosts = net::host_addresses(settings.ipv6);
        Self::with_host_addresses(settings, hosts)
    }

    /// An SFU whose rooms bind their media sockets on exactly `hosts` (tests use loopback).
    pub fn with_host_addresses(settings: Settings, hosts: Vec<IpAddr>) -> Self {
        Self {
            settings,
            hosts,
            rooms: Arc::new(Mutex::new(HashMap::new())),
            next_instance: AtomicU64::new(0),
        }
    }

    /// What the browser needs to know: whether media reaches the server directly or via TURN.
    pub fn client_info(&self) -> Value {
        json!({ "transport": if self.settings.relay.is_some() { "relay" } else { "direct" } })
    }

    /// Registers `peer_id` at `room_id`; events for it go to `events`. When the sender's
    /// receiver is dropped the SFU treats the seat as gone, as if [`Sfu::leave`] were called.
    /// Joining again under the same peer id replaces the earlier connection.
    pub async fn join(
        &self,
        room_id: &str,
        peer_id: &str,
        spectator: bool,
        events: mpsc::UnboundedSender<SfuEvent>,
    ) -> Result<(), SfuError> {
        let relay = match &self.settings.relay {
            Some(fetch) => Some(self.turn_servers(fetch().await).await),
            None => None,
        };
        let (reply, response) = oneshot::channel();
        let command = Command::Join {
            peer: PeerId::from(peer_id),
            spectator,
            events,
            relay,
            reply,
        };
        self.send(&RoomId::from(room_id), command, true)?;
        response
            .await
            .map_err(|_| SfuError::Unavailable("the room stopped".into()))?
    }

    /// The seat's channel ended.
    #[expect(
        clippy::unused_async,
        reason = "the channel's interface awaits every SFU call"
    )]
    pub async fn leave(&self, room_id: &str, peer_id: &str) {
        let _ = self.send(
            &RoomId::from(room_id),
            Command::Leave {
                peer: PeerId::from(peer_id),
            },
            false,
        );
    }

    /// Applies the browser's initial offer and returns the answer SDP.
    pub async fn offer(&self, room_id: &str, peer_id: &str, sdp: &str) -> Result<String, SfuError> {
        self.call(room_id, |reply| Command::Offer {
            peer: PeerId::from(peer_id),
            sdp: sdp.to_owned(),
            reply,
        })
        .await
    }

    /// Applies the browser's answer to a server offer.
    pub async fn answer(&self, room_id: &str, peer_id: &str, sdp: &str) -> Result<(), SfuError> {
        self.call(room_id, |reply| Command::Answer {
            peer: PeerId::from(peer_id),
            sdp: sdp.to_owned(),
            reply,
        })
        .await
    }

    /// Adds an ICE candidate (its JSON map) from the browser.
    pub async fn candidate(
        &self,
        room_id: &str,
        peer_id: &str,
        candidate: &Value,
    ) -> Result<(), SfuError> {
        self.call(room_id, |reply| Command::Candidate {
            peer: PeerId::from(peer_id),
            candidate: candidate.clone(),
            reply,
        })
        .await
    }

    /// Asks for `layer` of `owner_id`'s video, from how large `peer_id` draws it.
    pub async fn layer(
        &self,
        room_id: &str,
        peer_id: &str,
        owner_id: &str,
        layer: &str,
    ) -> Result<(), SfuError> {
        let layer =
            Layer::from_rid(layer).ok_or_else(|| SfuError::Rejected("invalid layer".into()))?;
        self.call(room_id, |reply| Command::Layer {
            peer: PeerId::from(peer_id),
            owner: PeerId::from(owner_id),
            layer,
            reply,
        })
        .await
    }

    /// Limits `peer_id`'s video to `target` (another peer id), or to everyone with `None`.
    pub async fn reveal(
        &self,
        room_id: &str,
        peer_id: &str,
        target: Option<&str>,
    ) -> Result<(), SfuError> {
        self.call(room_id, |reply| Command::Reveal {
            peer: PeerId::from(peer_id),
            target: target.map(PeerId::from),
            reply,
        })
        .await
    }

    /// Delivers `message` to `to`'s channel as [`SfuEvent::PeerMessage`].
    pub async fn relay(
        &self,
        room_id: &str,
        from: &str,
        to: &str,
        message: Value,
    ) -> Result<(), SfuError> {
        self.call(room_id, |reply| Command::Relay {
            from: PeerId::from(from),
            to: PeerId::from(to),
            message,
            reply,
        })
        .await
    }

    async fn call<T>(
        &self,
        room_id: &str,
        command: impl FnOnce(room::Reply<T>) -> Command,
    ) -> Result<T, SfuError> {
        let (reply, response) = oneshot::channel();
        self.send(&RoomId::from(room_id), command(reply), false)?;
        response.await.map_err(|_| SfuError::NotJoined)?
    }

    /// Hands `command` to the room, starting it when `create` is set. A room stops as soon as
    /// its last seat leaves, removing itself from the registry under the same lock.
    fn send(&self, room_id: &RoomId, command: Command, create: bool) -> Result<(), SfuError> {
        let mut rooms = self.rooms.lock().unwrap_or_else(PoisonError::into_inner);
        let command = match rooms.get(room_id) {
            Some(entry) => match entry.commands.send(command) {
                Ok(()) => return Ok(()),
                // The room's task ended without deregistering (it panicked); start another.
                Err(mpsc::error::SendError(command)) => {
                    rooms.remove(room_id);
                    command
                }
            },
            None => command,
        };
        if !create {
            return Err(SfuError::NotJoined);
        }
        let entry = self.start_room(room_id)?;
        entry
            .commands
            .send(command)
            .map_err(|_| SfuError::Unavailable("the room did not start".into()))?;
        rooms.insert(room_id.clone(), entry);
        Ok(())
    }

    fn start_room(&self, room_id: &RoomId) -> Result<RoomEntry, SfuError> {
        let (datagrams, datagram_receiver) = mpsc::channel(DATAGRAM_BACKLOG);
        let transport = if self.settings.relay.is_some() {
            Transport::Relay
        } else {
            self.direct_transport(&datagrams)?
        };
        let (commands, command_receiver) = mpsc::unbounded_channel();
        let room = Room::new(room_id.clone(), transport, commands.downgrade(), datagrams);
        let instance = self.next_instance.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(room::run(
            room,
            command_receiver,
            datagram_receiver,
            Arc::downgrade(&self.rooms),
            instance,
        ));
        Ok(RoomEntry { instance, commands })
    }

    /// One socket per interface address from the port range, announced as a host candidate,
    /// plus a server-reflexive candidate at `public_ip` (the router's WAN address) when set.
    fn direct_transport(
        &self,
        datagrams: &mpsc::Sender<net::Datagram>,
    ) -> Result<Transport, SfuError> {
        let mut sockets = Vec::new();
        let mut candidates = Vec::new();
        for ip in &self.hosts {
            let socket =
                match net::bind_in_range(*ip, self.settings.port_min, self.settings.port_max) {
                    Ok(socket) => socket,
                    Err(error) => {
                        tracing::warn!(
                            "SFU found no free UDP port on {ip} in {}..={}: {error}",
                            self.settings.port_min,
                            self.settings.port_max
                        );
                        continue;
                    }
                };
            let socket = net::ReadSocket::start(socket, datagrams.clone())
                .map_err(|error| SfuError::Unavailable(error.to_string()))?;
            let local = socket.local;
            if let Ok(candidate) = Candidate::host(local, "udp") {
                candidates.push(candidate);
            }
            if let Some(public) = self
                .settings
                .public_ip
                .filter(|public| public.is_ipv4() == ip.is_ipv4() && public != ip)
            {
                match Candidate::server_reflexive(
                    SocketAddr::new(public, local.port()),
                    local,
                    "udp",
                ) {
                    Ok(candidate) => candidates.push(candidate),
                    Err(error) => tracing::warn!("SFU cannot announce {public}: {error}"),
                }
            }
            sockets.push(socket);
        }
        if sockets.is_empty() {
            tracing::error!("SFU could not bind a media socket");
            return Err(SfuError::Unavailable("no free UDP port for media".into()));
        }
        Ok(Transport::Direct {
            sockets,
            candidates,
        })
    }

    /// The UDP TURN servers among `servers`, resolved (`relay_servers/0`: only entries with
    /// credentials count).
    async fn turn_servers(&self, servers: Vec<IceServer>) -> Vec<TurnServer> {
        let mut resolved = Vec::new();
        for server in servers {
            let (Some(username), Some(credential)) = (server.username, server.credential) else {
                continue;
            };
            let Some((host, port)) = server.urls.iter().find_map(|url| turn::udp_turn_host(url))
            else {
                continue;
            };
            let address = match tokio::net::lookup_host((host.as_str(), port)).await {
                Ok(mut addresses) => {
                    addresses.find(|address| self.settings.ipv6 || address.is_ipv4())
                }
                Err(error) => {
                    tracing::warn!("SFU could not resolve TURN server {host}: {error}");
                    None
                }
            };
            if let Some(address) = address {
                resolved.push(TurnServer {
                    address,
                    username: username.clone(),
                    credential: credential.clone(),
                });
            }
        }
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>(_value: &T) {}

    #[test]
    fn the_sfu_and_its_futures_can_cross_threads() {
        let sfu = Sfu::with_host_addresses(
            Settings {
                port_min: 0,
                port_max: 0,
                public_ip: None,
                ipv6: false,
                relay: None,
            },
            Vec::new(),
        );
        assert_send(&sfu);
        let (events, _receiver) = mpsc::unbounded_channel();
        assert_send(&sfu.join("r", "p", false, events));
        assert_send(&sfu.leave("r", "p"));
        assert_send(&sfu.offer("r", "p", ""));
        assert_send(&sfu.answer("r", "p", ""));
        assert_send(&sfu.candidate("r", "p", &Value::Null));
        assert_send(&sfu.layer("r", "p", "o", "m"));
        assert_send(&sfu.reveal("r", "p", None));
        assert_send(&sfu.relay("r", "p", "o", Value::Null));
    }

    #[test]
    fn layers_are_the_three_simulcast_rids() {
        assert!(LAYERS.iter().all(|layer| valid_layer(layer)));
        assert!(!valid_layer("x"));
        assert_eq!(
            LAYERS.map(|rid| Layer::from_rid(rid).map(Layer::rid)),
            LAYERS.map(Some)
        );
    }

    #[tokio::test]
    async fn a_room_that_cannot_bind_a_socket_is_unavailable() {
        let sfu = Sfu::with_host_addresses(
            Settings {
                port_min: 10,
                port_max: 9,
                public_ip: None,
                ipv6: false,
                relay: None,
            },
            vec!["127.0.0.1".parse().unwrap()],
        );
        let (events, _receiver) = mpsc::unbounded_channel();
        assert!(matches!(
            sfu.join("r", "p", false, events).await,
            Err(SfuError::Unavailable(_))
        ));
    }
}
