//! The SFU for one webcam table (`TheGathering.WebcamTables.Sfu.Room`): a server-side str0m
//! `Rtc` per connected browser, and the forwarding of every publisher's chosen simulcast layer
//! to every other connection.
//!
//! Signaling runs over the table's channel. The browser makes the first offer (its camera as
//! three layers); from then on only the server offers, adding or stopping a `sendonly` media
//! section per other publisher, and the browser answers. Messages for a browser go to its
//! channel's event sender as [`SfuEvent`]s.
//!
//! A room is one task owning every connection at the table and the room's UDP sockets, so
//! forwarding a packet is a plain function call. It stops as soon as its last seat leaves.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::media::{Direction, KeyframeRequestKind, MediaKind, Mid, Rid, Rids};
use str0m::net::{DatagramRecv, Protocol, Receive, Transmit};
use str0m::rtp::{ExtensionValues, RtpPacket, RtpWrite, SeqNo, Vp8Descriptor};
use str0m::{Candidate, CandidateKind, Event, IceConnectionState, Input, Output, Rtc};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::codec::CodecParams;
use crate::ice_report::{
    self, Address, CandidateStats, Entry, IceStats, PairStats, TransportStats,
};
use crate::ids::{PeerId, RoomId};
use crate::layer::{Encoding, Layer};
use crate::munger::{RtpIn, RtpOut};
use crate::net::{Datagram, ReadSocket};
use crate::simulcast_sdp::{self, AttrsByMid};
use crate::subscription::{Route, Subscription, nearest_live};
use crate::turn::{TurnAllocation, TurnInput, TurnServer};
use crate::{SfuError, SfuEvent, browser_sdp};

/// A keyframe request per publisher layer at most this often; a browser answering every PLI
/// from several viewers at once would spend its whole bitrate on keyframes.
const PLI_INTERVAL: Duration = Duration::from_millis(300);

/// Browsers pause simulcast layers their uplink cannot carry and bring them back as the
/// estimate recovers. A layer silent this long counts as paused, and a viewer on it is moved
/// to one still arriving; it returns once the wanted layer has been back this long, so a
/// layer that keeps flapping does not drag the viewer back and forth.
const ADAPT_INTERVAL: Duration = Duration::from_millis(500);
const STALE: Duration = Duration::from_millis(500);
const RECOVERED: Duration = Duration::from_secs(2);

/// str0m's ICE agent reports `disconnected` as soon as its checks stop succeeding and never
/// reports `failed`; a connection that has not been connected for this long counts as failed
/// (`ex_ice` gave up 8 s after it last heard from the browser).
const FAILED_AFTER: Duration = Duration::from_secs(10);

/// An ICE restart keeps the DTLS session, tracks and subscriptions and only redoes the path;
/// the seat is dropped only when restarts keep failing.
const MAX_ICE_RESTARTS: usize = 3;
const ICE_RESTART_WINDOW: Duration = Duration::from_secs(120);

/// Answers that may fail in a row before the seat is told to reconnect.
const MAX_FAILED_ANSWERS: u32 = 3;

/// The H.264 entry offered: Constrained Baseline 3.1 in packetization mode 1, which every
/// browser encodes and decodes (in hardware where it can), so any publisher's stream plays at
/// every viewer. Offering more profiles lets a publisher pick one some viewer cannot decode.
const H264_PROFILE_LEVEL_ID: u32 = 0x0042_e01f;
const H264_PT: u8 = 108;
const H264_RTX_PT: u8 = 109;

pub(crate) type Reply<T> = oneshot::Sender<Result<T, SfuError>>;

/// What the room is asked to do, by the [`crate::Sfu`] methods and its own watchers.
pub(crate) enum Command {
    Join {
        peer: PeerId,
        spectator: bool,
        events: mpsc::UnboundedSender<SfuEvent>,
        relay: Option<Vec<TurnServer>>,
        reply: Reply<()>,
    },
    Leave {
        peer: PeerId,
    },
    Offer {
        peer: PeerId,
        sdp: String,
        reply: Reply<String>,
    },
    Answer {
        peer: PeerId,
        sdp: String,
        reply: Reply<()>,
    },
    Candidate {
        peer: PeerId,
        candidate: Value,
        reply: Reply<()>,
    },
    Layer {
        peer: PeerId,
        owner: PeerId,
        layer: Layer,
        reply: Reply<()>,
    },
    Reveal {
        peer: PeerId,
        target: Option<PeerId>,
        reply: Reply<()>,
    },
    Relay {
        from: PeerId,
        to: PeerId,
        message: Value,
        reply: Reply<()>,
    },
    /// The channel's event receiver was dropped.
    ChannelClosed {
        peer: PeerId,
        generation: u64,
    },
}

/// The registry entry for a running room.
pub(crate) struct RoomEntry {
    pub(crate) instance: u64,
    pub(crate) commands: mpsc::UnboundedSender<Command>,
}

pub(crate) type Registry = Mutex<HashMap<RoomId, RoomEntry>>;

/// How the room's connections reach browsers.
pub(crate) enum Transport {
    /// Host (and server-reflexive) candidates on the room's own sockets.
    Direct {
        sockets: Vec<ReadSocket>,
        candidates: Vec<Candidate>,
    },
    /// Only relayed candidates, allocated per connection on the configured TURN servers.
    Relay,
}

/// What a seat publishes.
struct Publisher {
    mid: Mid,
    /// Simulcast layers, softest first; `None` for a camera without simulcast.
    rids: Option<Vec<Encoding>>,
    codec: Option<CodecParams>,
    layers: HashMap<Encoding, LayerSeen>,
}

#[derive(Clone, Copy)]
struct LayerSeen {
    last: Instant,
    since: Instant,
}

/// A server offer the browser has not answered yet.
struct Negotiation {
    pending: SdpPendingOffer,
    /// Media sections the offer added, by board owner.
    added: Vec<(PeerId, Mid)>,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "independent negotiation flags, as in the Elixir room's peer map"
)]
struct Peer {
    /// Which join this is; a rejoin under the same id gets a new one.
    generation: u64,
    events: mpsc::UnboundedSender<SfuEvent>,
    watcher: JoinHandle<()>,
    spectator: bool,
    rtc: Rtc,
    next_timeout: Instant,
    reveal_to: Option<PeerId>,
    publisher: Option<Publisher>,
    /// Boards this connection receives, by owner.
    subs: BTreeMap<PeerId, Subscription>,
    sub_mids: HashMap<Mid, PeerId>,
    ready: bool,
    negotiation: Option<Negotiation>,
    dirty: bool,
    restart_ice: bool,
    ice_restarts: Vec<Instant>,
    failed_answers: u32,
    /// Media sections to stop in the next offer.
    to_stop: Vec<Mid>,
    /// Browser candidates that arrived before its offer.
    candidates: Vec<Candidate>,
    simulcast: AttrsByMid,
    /// The browser's first media section, which trickled candidates are addressed to.
    first_mid: Option<String>,
    turn: Vec<TurnAllocation>,
    ice: IceLog,
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

/// What the room saw of a connection's ICE, for failure detection and the ICE report.
struct IceLog {
    state: IceConnectionState,
    dtls_connected: bool,
    ever_connected: bool,
    /// Since when the connection has not been connected, once it has an answer.
    unconnected_since: Option<Instant>,
    packets_received: u64,
    packets_sent: u64,
    destination: Option<SocketAddr>,
    destination_changes: u64,
    local: Vec<Candidate>,
    remote: Vec<Candidate>,
    /// mDNS host candidates the browser trickled (name and port), which str0m cannot use.
    mdns: Vec<(String, u16)>,
    heard: BTreeMap<SocketAddr, (SocketAddr, Instant)>,
}

pub(crate) struct Room {
    id: RoomId,
    transport: Transport,
    peers: BTreeMap<PeerId, Peer>,
    queue: VecDeque<(PeerId, u64, Event)>,
    crashed: Vec<(PeerId, u64, String)>,
    plis: HashMap<(PeerId, Encoding), Instant>,
    next_generation: u64,
    next_adapt: Instant,
    started: Instant,
    commands: mpsc::WeakUnboundedSender<Command>,
    datagrams: mpsc::Sender<Datagram>,
}

/// Runs `room` until its last seat leaves (or the SFU is dropped).
pub(crate) async fn run(
    mut room: Room,
    mut commands: mpsc::UnboundedReceiver<Command>,
    mut datagrams: mpsc::Receiver<Datagram>,
    registry: Weak<Registry>,
    instance: u64,
) {
    loop {
        let deadline = tokio::time::Instant::from_std(room.next_deadline(Instant::now()));
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => room.handle_command(command),
                None => break,
            },
            Some(datagram) = datagrams.recv() => room.handle_datagram(&datagram),
            () = tokio::time::sleep_until(deadline) => room.handle_timeout(Instant::now()),
        }

        if room.peers.is_empty() {
            // Stop, unless a command (a join) is already on its way: the registry lock makes
            // the check and the removal atomic with respect to `Sfu` sending one.
            let Some(registry) = registry.upgrade() else {
                break;
            };
            let mut rooms = registry.lock().unwrap_or_else(PoisonError::into_inner);
            if commands.is_empty() {
                if rooms
                    .get(&room.id)
                    .is_some_and(|entry| entry.instance == instance)
                {
                    rooms.remove(&room.id);
                }
                tracing::debug!("SFU room {} stopped", room.id);
                break;
            }
        }
    }
}

impl Room {
    pub(crate) fn new(
        id: RoomId,
        transport: Transport,
        commands: mpsc::WeakUnboundedSender<Command>,
        datagrams: mpsc::Sender<Datagram>,
    ) -> Self {
        let now = Instant::now();
        Self {
            id,
            transport,
            peers: BTreeMap::new(),
            queue: VecDeque::new(),
            crashed: Vec::new(),
            plis: HashMap::new(),
            next_generation: 0,
            next_adapt: now + ADAPT_INTERVAL,
            started: now,
            commands,
            datagrams,
        }
    }

    fn next_deadline(&self, now: Instant) -> Instant {
        let mut deadline = (now + Duration::from_secs(1)).min(self.next_adapt);
        for peer in self.peers.values() {
            deadline = deadline.min(peer.next_timeout);
            if let Some(since) = peer.ice.unconnected_since {
                deadline = deadline.min(since + FAILED_AFTER);
            }
            for allocation in &peer.turn {
                deadline = deadline.min(allocation.next_timeout());
            }
        }
        deadline.max(now)
    }

    // --- Commands ---------------------------------------------------------------------------

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Join {
                peer,
                spectator,
                events,
                relay,
                reply,
            } => {
                let result = self.join(&peer, spectator, events, relay);
                let _ = reply.send(result);
            }
            Command::Leave { peer } => {
                if self.peers.contains_key(&peer) {
                    tracing::info!("SFU seat {peer} left table {}", self.id);
                    self.remove_peer(&peer);
                }
            }
            Command::Offer { peer, sdp, reply } => {
                let result = self.offer(&peer, &sdp);
                let _ = reply.send(result);
            }
            Command::Answer { peer, sdp, reply } => {
                let result = self.answer(&peer, &sdp);
                let _ = reply.send(result);
            }
            Command::Candidate {
                peer,
                candidate,
                reply,
            } => {
                let result = self.candidate(&peer, &candidate);
                let _ = reply.send(result);
            }
            Command::Layer {
                peer,
                owner,
                layer,
                reply,
            } => {
                let result = self.layer(&peer, &owner, layer);
                let _ = reply.send(result);
            }
            Command::Reveal {
                peer,
                target,
                reply,
            } => {
                let result = self.reveal(&peer, target);
                let _ = reply.send(result);
            }
            Command::Relay {
                from,
                to,
                message,
                reply,
            } => {
                let result = match self.peers.get(&to) {
                    Some(peer) => {
                        let _ = peer.events.send(SfuEvent::PeerMessage(
                            json!({ "from": from.to_string(), "message": message }),
                        ));
                        Ok(())
                    }
                    None => Err(SfuError::NotJoined),
                };
                let _ = reply.send(result);
            }
            Command::ChannelClosed { peer, generation } => {
                if self
                    .peers
                    .get(&peer)
                    .is_some_and(|p| p.generation == generation)
                {
                    tracing::info!("SFU seat {peer} left table {} (channel closed)", self.id);
                    self.remove_peer(&peer);
                }
            }
        }
        self.process();
    }

    fn join(
        &mut self,
        id: &PeerId,
        spectator: bool,
        events: mpsc::UnboundedSender<SfuEvent>,
        relay: Option<Vec<TurnServer>>,
    ) -> Result<(), SfuError> {
        if self.peers.contains_key(id) {
            self.remove_peer(id);
        }
        let now = Instant::now();
        let mut rtc = new_rtc(now);

        let mut local = Vec::new();
        let mut turn = Vec::new();
        match (&self.transport, relay) {
            (Transport::Direct { candidates, .. }, _) => {
                for candidate in candidates {
                    if let Some(added) = rtc.add_local_candidate(candidate.clone()) {
                        local.push(added.clone());
                    }
                }
            }
            (Transport::Relay, servers) => {
                for server in servers.unwrap_or_default() {
                    match TurnAllocation::start(&server, self.datagrams.clone(), now) {
                        Ok(allocation) => turn.push(allocation),
                        Err(error) => tracing::warn!(
                            "SFU could not reach TURN server {}: {error}",
                            server.address
                        ),
                    }
                }
                if turn.is_empty() {
                    tracing::error!("SFU has no usable TURN server for relay-only operation");
                    return Err(SfuError::Unavailable("no TURN server".into()));
                }
            }
        }

        let generation = self.next_generation;
        self.next_generation += 1;
        let watcher = {
            let events = events.clone();
            let commands = self.commands.clone();
            let peer = id.clone();
            tokio::spawn(async move {
                events.closed().await;
                if let Some(commands) = commands.upgrade() {
                    let _ = commands.send(Command::ChannelClosed { peer, generation });
                }
            })
        };

        let peer = Peer {
            generation,
            events,
            watcher,
            spectator,
            rtc,
            next_timeout: now,
            reveal_to: None,
            publisher: None,
            subs: BTreeMap::new(),
            sub_mids: HashMap::new(),
            ready: false,
            negotiation: None,
            dirty: false,
            restart_ice: false,
            ice_restarts: Vec::new(),
            failed_answers: 0,
            to_stop: Vec::new(),
            candidates: Vec::new(),
            simulcast: AttrsByMid::new(),
            first_mid: None,
            turn,
            ice: IceLog {
                state: IceConnectionState::New,
                dtls_connected: false,
                ever_connected: false,
                unconnected_since: None,
                packets_received: 0,
                packets_sent: 0,
                destination: None,
                destination_changes: 0,
                local,
                remote: Vec::new(),
                mdns: Vec::new(),
                heard: BTreeMap::new(),
            },
        };
        self.peers.insert(id.clone(), peer);
        self.flush_turn(id, now);
        let role = if spectator { "spectator" } else { "seat" };
        tracing::info!("SFU {role} {id} joined table {}", self.id);
        Ok(())
    }

    fn offer(&mut self, id: &PeerId, sdp: &str) -> Result<String, SfuError> {
        let peer = self.peers.get_mut(id).ok_or(SfuError::NotJoined)?;
        let offer =
            SdpOffer::from_sdp_string(&browser_sdp::unify_dtls_roles(sdp)).map_err(|error| {
                tracing::warn!("SFU rejected an offer from {id}: {error}");
                SfuError::Rejected(error.to_string())
            })?;
        // A second offer (the browser never sends one) supersedes any outstanding server offer.
        if let Some(negotiation) = peer.negotiation.take() {
            revert(peer, &negotiation.added);
            peer.dirty = true;
        }
        let answer = match peer.rtc.sdp_api().accept_offer(offer) {
            Ok(answer) => answer,
            Err(error) => {
                tracing::warn!("SFU rejected an offer from {id}: {error}");
                return Err(SfuError::Rejected(error.to_string()));
            }
        };
        let answer = answer.to_sdp_string();
        self.drive(id);

        let now = Instant::now();
        if let Some(peer) = self.peers.get_mut(id) {
            // Candidates in the offer itself need relay permissions as much as trickled ones.
            if !peer.turn.is_empty() {
                let offered = sdp
                    .lines()
                    .filter_map(|line| line.strip_prefix("a="))
                    .filter_map(|line| Candidate::from_sdp_string(line.trim()).ok());
                for candidate in offered {
                    for allocation in &mut peer.turn {
                        allocation.permit(candidate.addr().ip(), now);
                    }
                }
            }
            for candidate in std::mem::take(&mut peer.candidates) {
                peer.rtc.add_remote_candidate(candidate);
            }
            peer.ready = true;
            peer.simulcast = simulcast_sdp::receiving(sdp);
            peer.first_mid = browser_sdp::split_sections(sdp)
                .media
                .first()
                .and_then(|section| browser_sdp::attribute(section, "mid"))
                .map(str::to_owned);
            if !peer.ice.state.is_connected() {
                peer.ice.unconnected_since.get_or_insert(now);
            }
        }
        self.drive(id);
        self.flush_turn(id, now);
        self.subscribe_to_publishers(id);
        self.negotiate(id);
        self.publish_from_offer(id, sdp);
        Ok(answer)
    }

    fn answer(&mut self, id: &PeerId, sdp: &str) -> Result<(), SfuError> {
        let peer = self.peers.get_mut(id).ok_or(SfuError::NotJoined)?;
        let Some(negotiation) = peer.negotiation.take() else {
            tracing::warn!("SFU got an answer from {id} without an open offer");
            return Err(SfuError::Rejected("unexpected answer".into()));
        };
        let result = SdpAnswer::from_sdp_string(&browser_sdp::unify_dtls_roles(sdp))
            .map_err(|error| error.to_string())
            .and_then(|answer| {
                peer.rtc
                    .sdp_api()
                    .accept_answer(negotiation.pending, answer)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(()) => {
                peer.failed_answers = 0;
                self.drive(id);
                self.apply_codecs(id);
                self.negotiate(id);
                Ok(())
            }
            Err(reason) => {
                // `ex_webrtc` left the room waiting for an answer forever after a rejected one,
                // so the seat never got another offer. The offered changes are rolled back and
                // offered again; a browser that keeps failing reconnects.
                tracing::warn!("SFU rejected an answer from {id}: {reason}");
                revert(peer, &negotiation.added);
                peer.failed_answers += 1;
                if peer.failed_answers >= MAX_FAILED_ANSWERS {
                    self.disconnect_peer(id, "negotiation failed");
                } else {
                    peer.dirty = true;
                    self.negotiate(id);
                }
                Err(SfuError::Rejected(reason))
            }
        }
    }

    fn candidate(&mut self, id: &PeerId, json: &Value) -> Result<(), SfuError> {
        let peer = self.peers.get_mut(id).ok_or(SfuError::NotJoined)?;
        let Some(line) = json.get("candidate").and_then(Value::as_str) else {
            return Err(SfuError::Rejected("invalid candidate".into()));
        };
        let line = line.trim().trim_start_matches("a=");
        if line.is_empty() {
            // End of candidates.
            return Ok(());
        }
        let candidate = match Candidate::from_sdp_string(line) {
            Ok(candidate) => candidate,
            Err(error) if line.contains(".local ") => {
                // Browsers hide host addresses behind mDNS names str0m cannot resolve; the
                // browser's checks from that address still arrive as a peer-reflexive pair.
                tracing::debug!("SFU ignored an mDNS candidate from {id}: {error}");
                let mut fields = line.split_whitespace().skip(4);
                if let (Some(name), Some(port)) = (
                    fields.next(),
                    fields.next().and_then(|port| port.parse().ok()),
                ) {
                    peer.ice.mdns.push((name.to_owned(), port));
                }
                return Ok(());
            }
            Err(error) => {
                tracing::debug!("SFU rejected a candidate from {id}: {error}");
                return Err(SfuError::Rejected("invalid candidate".into()));
            }
        };
        tracing::debug!("SFU candidate from {id}: {line}");
        peer.ice.remote.push(candidate.clone());
        for allocation in &mut peer.turn {
            allocation.permit(candidate.addr().ip(), Instant::now());
        }
        if peer.ready {
            peer.rtc.add_remote_candidate(candidate);
            self.drive(id);
        } else {
            peer.candidates.push(candidate);
        }
        self.flush_turn(id, Instant::now());
        Ok(())
    }

    fn layer(&mut self, id: &PeerId, owner_id: &PeerId, layer: Layer) -> Result<(), SfuError> {
        if !self.peers.contains_key(id) {
            return Err(SfuError::NotJoined);
        }
        let owner = self.peers.get(owner_id).ok_or(SfuError::NotJoined)?;
        // A publisher without simulcast has nothing to choose from.
        let Some(rids) = owner
            .publisher
            .as_ref()
            .and_then(|publisher| publisher.rids.clone())
        else {
            return Ok(());
        };
        let viewer = self.peers.get_mut(id).ok_or(SfuError::NotJoined)?;
        let sub = viewer
            .subs
            .get_mut(owner_id)
            .ok_or_else(|| SfuError::Rejected("not subscribed".into()))?;
        let wanted = Encoding::Layer(layer);
        let layer = if rids.contains(&wanted) {
            wanted
        } else {
            rids.last().copied().unwrap_or(wanted)
        };
        if sub.request_layer(layer) {
            self.request_keyframe(owner_id, Some(layer));
        }
        Ok(())
    }

    fn reveal(&mut self, id: &PeerId, target: Option<PeerId>) -> Result<(), SfuError> {
        let peer = self.peers.get_mut(id).ok_or(SfuError::NotJoined)?;
        peer.reveal_to = target;
        self.refresh_allowed(id);
        Ok(())
    }

    // --- Network and time -------------------------------------------------------------------

    fn handle_datagram(&mut self, datagram: &Datagram) {
        let at = datagram.at;
        // Traffic from a TURN server is unwrapped by the allocation it belongs to.
        let turn_owner = self.peers.iter().find_map(|(id, peer)| {
            peer.turn
                .iter()
                .position(|allocation| allocation.socket_addr() == datagram.local)
                .map(|index| (id.clone(), index))
        });
        if let Some((id, index)) = turn_owner {
            let input = self
                .peers
                .get_mut(&id)
                .and_then(|peer| peer.turn.get_mut(index))
                .map(|allocation| allocation.handle(datagram.source, &datagram.data, at));
            match input {
                Some(TurnInput::Relayed {
                    relayed,
                    source,
                    data,
                }) => {
                    self.receive(Some(&id), relayed, source, &data, at);
                }
                Some(TurnInput::Allocated(candidate)) => self.add_relay_candidate(&id, candidate),
                Some(TurnInput::Nothing) | None => {}
            }
            self.flush_turn(&id, at);
        } else {
            self.receive(None, datagram.local, datagram.source, &datagram.data, at);
        }
        self.process();
    }

    /// Feeds one datagram to the connection it belongs to (`only`, when already known).
    fn receive(
        &mut self,
        only: Option<&PeerId>,
        local: SocketAddr,
        source: SocketAddr,
        data: &[u8],
        at: Instant,
    ) {
        let Ok(contents) = DatagramRecv::try_from(data) else {
            return;
        };
        let input = Input::Receive(
            at,
            Receive {
                proto: Protocol::Udp,
                source,
                destination: local,
                contents,
            },
        );
        let id = match only {
            Some(id) => Some(id.clone()),
            None => self
                .peers
                .iter()
                .find(|(_, peer)| peer.rtc.accepts(&input))
                .map(|(id, _)| id.clone()),
        };
        let Some(id) = id else {
            tracing::debug!(
                "SFU room {} dropped a datagram from {source} no connection accepts",
                self.id
            );
            return;
        };
        let Some(peer) = self.peers.get_mut(&id) else {
            return;
        };
        peer.ice.packets_received += 1;
        peer.ice.heard.insert(source, (local, at));
        if let Err(error) = peer.rtc.handle_input(input) {
            self.crashed
                .push((id.clone(), peer.generation, error.to_string()));
        }
        self.drive(&id);
    }

    fn handle_timeout(&mut self, now: Instant) {
        let due: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(_, peer)| peer.next_timeout <= now)
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            if let Some(peer) = self.peers.get_mut(&id)
                && let Err(error) = peer.rtc.handle_input(Input::Timeout(now))
            {
                self.crashed
                    .push((id.clone(), peer.generation, error.to_string()));
            }
            self.drive(&id);
        }

        let turn: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(_, peer)| {
                peer.turn
                    .iter()
                    .any(|allocation| allocation.next_timeout() <= now)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in turn {
            self.flush_turn(&id, now);
        }

        if now >= self.next_adapt {
            self.next_adapt = now + ADAPT_INTERVAL;
            self.adapt_layers(now);
        }

        let failed: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(_, peer)| {
                peer.ice
                    .unconnected_since
                    .is_some_and(|since| now >= since + FAILED_AFTER)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in failed {
            self.connection_failed(&id, now);
        }
        self.process();
    }

    /// Drains the connection's output: datagrams go out, events wait in the queue.
    fn drive(&mut self, id: &PeerId) {
        let Some(peer) = self.peers.get_mut(id) else {
            return;
        };
        loop {
            match peer.rtc.poll_output() {
                Ok(Output::Timeout(at)) => {
                    peer.next_timeout = at;
                    break;
                }
                Ok(Output::Transmit(transmit)) => send(&self.transport, peer, &transmit),
                Ok(Output::Event(event)) => {
                    self.queue.push_back((id.clone(), peer.generation, event));
                }
                Err(error) => {
                    self.crashed
                        .push((id.clone(), peer.generation, error.to_string()));
                    break;
                }
            }
        }
    }

    /// Sends what the connection's TURN allocations have queued, and applies their results.
    fn flush_turn(&mut self, id: &PeerId, now: Instant) {
        let Some(peer) = self.peers.get_mut(id) else {
            return;
        };
        for allocation in &mut peer.turn {
            allocation.poll(now);
        }
    }

    fn add_relay_candidate(&mut self, id: &PeerId, candidate: Candidate) {
        let Some(peer) = self.peers.get_mut(id) else {
            return;
        };
        let Some(added) = peer.rtc.add_local_candidate(candidate).cloned() else {
            return;
        };
        tracing::debug!("SFU relay candidate for {id}: {added}");
        peer.ice.local.push(added.clone());
        // The answer may already have gone out without it; trickle it like `ex_webrtc` did.
        if peer.ready {
            let _ = peer.events.send(SfuEvent::Candidate(
                json!({ "candidate": candidate_json(&added, peer) }),
            ));
        }
        self.drive(id);
    }

    /// Handles every queued event, and every connection that failed meanwhile.
    fn process(&mut self) {
        loop {
            if let Some((id, generation, reason)) = self.crashed.pop() {
                if self
                    .peers
                    .get(&id)
                    .is_some_and(|peer| peer.generation == generation)
                {
                    tracing::warn!("SFU peer connection for {id} exited: {reason}");
                    self.disconnect_peer(&id, &reason);
                }
                continue;
            }
            let Some((id, generation, event)) = self.queue.pop_front() else {
                break;
            };
            if self
                .peers
                .get(&id)
                .is_some_and(|peer| peer.generation == generation)
            {
                self.handle_event(&id, event);
            }
        }
    }

    // --- WebRTC events ----------------------------------------------------------------------

    fn handle_event(&mut self, id: &PeerId, event: Event) {
        match event {
            // str0m reports remote media only once SRTP is up; the publisher was registered
            // from the offer already, so this only covers media added later.
            Event::MediaAdded(added) => {
                let rids = added.simulcast.map(|simulcast| {
                    simulcast
                        .recv
                        .iter()
                        .map(|layer| layer.rid)
                        .collect::<Vec<Rid>>()
                });
                self.publish(id, added.mid, added.kind, added.direction, rids.as_deref());
            }
            Event::RtpPacket(packet) => self.publisher_packet(id, &packet),
            // A viewer's decoder lost sync and asks for a keyframe; the publisher's layer it is
            // watching is the one that must send it.
            Event::KeyframeRequest(request) => {
                let Some(peer) = self.peers.get(id) else {
                    return;
                };
                let Some(owner) = peer.sub_mids.get(&request.mid).cloned() else {
                    return;
                };
                let layer = peer
                    .subs
                    .get(&owner)
                    .and_then(|sub| sub.layer.or(sub.pending));
                self.request_keyframe(&owner, layer);
            }
            Event::IceConnectionStateChange(state) => {
                let Some(peer) = self.peers.get_mut(id) else {
                    return;
                };
                tracing::debug!("SFU ICE for {id}: {state:?}");
                peer.ice.state = state;
                if state.is_connected() {
                    peer.ice.unconnected_since = None;
                } else if peer.ready {
                    peer.ice.unconnected_since.get_or_insert_with(Instant::now);
                }
            }
            Event::Connected => {
                if let Some(peer) = self.peers.get_mut(id) {
                    peer.ice.dtls_connected = true;
                    if !peer.ice.ever_connected {
                        peer.ice.ever_connected = true;
                        tracing::info!("SFU peer connection for {id} connected");
                    }
                }
            }
            Event::Closed => {
                tracing::info!("SFU peer connection for {id} closed by the browser");
                self.disconnect_peer(id, "closed");
            }
            _ => {}
        }
    }

    fn publisher_packet(&mut self, id: &PeerId, packet: &RtpPacket) {
        let Some(peer) = self.peers.get_mut(id) else {
            return;
        };
        let Some((mid, rid)) = peer
            .rtc
            .direct_api()
            .stream_rx(&packet.header.ssrc)
            .map(|stream| (stream.mid(), stream.rid()))
        else {
            return;
        };
        let Some(publisher) = peer
            .publisher
            .as_mut()
            .filter(|publisher| publisher.mid == mid)
        else {
            return;
        };
        let rid = match rid {
            None => Encoding::Single,
            Some(rid) => match Layer::from_rid(&rid) {
                Some(layer) => Encoding::Layer(layer),
                None => return,
            },
        };
        note_layer(publisher, rid, packet.timestamp);

        // The publisher's codec is whatever payload type its first packet carries.
        if publisher.codec.is_none() {
            let codec = peer
                .rtc
                .codec_config()
                .find(|params| params.pt() == packet.header.payload_type)
                .and_then(CodecParams::from_payload);
            let Some(codec) = codec else {
                return;
            };
            publisher.codec = Some(codec);
            let viewers: Vec<PeerId> = self
                .peers
                .keys()
                .filter(|viewer| *viewer != id)
                .cloned()
                .collect();
            for viewer in viewers {
                self.apply_codecs(&viewer);
            }
        }
        self.forward(id, rid, packet);
    }

    /// Registers the seat's camera as its board, once (`{:track, track}` in the Elixir room),
    /// and gives every other connection a subscription to it.
    fn publish(
        &mut self,
        id: &PeerId,
        mid: Mid,
        kind: MediaKind,
        direction: Direction,
        rids: Option<&[Rid]>,
    ) {
        let Some(peer) = self.peers.get_mut(id) else {
            return;
        };
        let receiving = matches!(direction, Direction::RecvOnly | Direction::SendRecv);
        if peer.spectator || peer.publisher.is_some() || kind != MediaKind::Video || !receiving {
            return;
        }
        let rids: Option<Vec<Encoding>> = rids
            .map(|rids| {
                rids.iter()
                    .filter_map(|rid| Layer::from_rid(rid))
                    .map(Encoding::Layer)
                    .collect()
            })
            .filter(|rids: &Vec<Encoding>| !rids.is_empty());
        peer.publisher = Some(Publisher {
            mid,
            rids,
            codec: None,
            layers: HashMap::new(),
        });
        let viewers: Vec<PeerId> = self
            .peers
            .keys()
            .filter(|viewer| *viewer != id)
            .cloned()
            .collect();
        for viewer in viewers {
            self.subscribe(&viewer, id);
            self.negotiate(&viewer);
        }
    }

    /// The first video section the browser's offer sends on.
    fn publish_from_offer(&mut self, id: &PeerId, sdp: &str) {
        let Some(peer) = self.peers.get(id) else {
            return;
        };
        let sections = browser_sdp::split_sections(sdp);
        let found = sections.media.iter().find_map(|section| {
            let mid = Mid::from(browser_sdp::attribute(section, "mid")?);
            let media = peer.rtc.media(mid)?;
            let receiving = matches!(media.direction(), Direction::RecvOnly | Direction::SendRecv);
            (media.kind() == MediaKind::Video && receiving && !media.disabled()).then(|| {
                let rids = match media.rids_rx() {
                    Rids::Specific(rids) => Some(rids.clone()),
                    Rids::None | Rids::Any => None,
                };
                (mid, media.direction(), rids)
            })
        });
        if let Some((mid, direction, rids)) = found {
            self.publish(id, mid, MediaKind::Video, direction, rids.as_deref());
        }
    }

    // --- Subscriptions ----------------------------------------------------------------------

    fn subscribe_to_publishers(&mut self, viewer: &PeerId) {
        let owners: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(id, peer)| *id != viewer && peer.publisher.is_some())
            .map(|(id, _)| id.clone())
            .collect();
        for owner in owners {
            self.subscribe(viewer, &owner);
        }
    }

    /// Gives `viewer_id` a subscription to `owner_id`'s video; its `sendonly` media section is
    /// added in the next offer, with the owner's peer id as the stream id so the browser can
    /// tell whose board arrived.
    fn subscribe(&mut self, viewer_id: &PeerId, owner_id: &PeerId) {
        let Some(owner) = self.peers.get(owner_id) else {
            return;
        };
        let Some(publisher) = &owner.publisher else {
            return;
        };
        let wanted = if publisher.rids.is_some() {
            Encoding::Layer(Layer::Medium)
        } else {
            Encoding::Single
        };
        let reveal_to = owner.reveal_to.clone();
        let Some(viewer) = self.peers.get_mut(viewer_id) else {
            return;
        };
        if viewer.subs.contains_key(owner_id) {
            return;
        }
        let mut sub = Subscription::new(wanted);
        sub.set_allowed(allowed(reveal_to.as_ref(), viewer_id));
        viewer.subs.insert(owner_id.clone(), sub);
        viewer.dirty = true;
    }

    fn unsubscribe(&mut self, viewer_id: &PeerId, owner_id: &PeerId) {
        let Some(viewer) = self.peers.get_mut(viewer_id) else {
            return;
        };
        let Some(sub) = viewer.subs.remove(owner_id) else {
            return;
        };
        if let Some(mid) = sub.mid {
            viewer.sub_mids.remove(&mid);
            viewer.to_stop.push(mid);
        }
        viewer.dirty = true;
        self.negotiate(viewer_id);
    }

    fn refresh_allowed(&mut self, owner_id: &PeerId) {
        let Some(owner) = self.peers.get(owner_id) else {
            return;
        };
        if owner.publisher.is_none() {
            return;
        }
        let reveal_to = owner.reveal_to.clone();
        let mut keyframes = Vec::new();
        for (viewer_id, viewer) in &mut self.peers {
            if let Some(sub) = viewer.subs.get_mut(owner_id)
                && sub.set_allowed(allowed(reveal_to.as_ref(), viewer_id))
            {
                keyframes.push(sub.pending);
            }
        }
        for layer in keyframes {
            self.request_keyframe(owner_id, layer);
        }
    }

    // --- Layer liveness ---------------------------------------------------------------------

    /// Moves viewers off publisher layers that have gone quiet and back once the wanted one
    /// has reliably returned. Only simulcast subscriptions already showing a layer take part;
    /// a blank one adopts whatever keyframe arrives first (see [`Subscription::route`]).
    fn adapt_layers(&mut self, now: Instant) {
        let mut moves = Vec::new();
        for (viewer_id, viewer) in &self.peers {
            for (owner_id, sub) in &viewer.subs {
                let Some(current @ Encoding::Layer(_)) = sub.layer else {
                    continue;
                };
                let Some(publisher) = self
                    .peers
                    .get(owner_id)
                    .and_then(|owner| owner.publisher.as_ref())
                else {
                    continue;
                };
                let Some(rids) = &publisher.rids else {
                    continue;
                };
                let live: Vec<Encoding> = publisher
                    .layers
                    .iter()
                    .filter(|(_, seen)| now.saturating_duration_since(seen.last) < STALE)
                    .map(|(rid, _)| *rid)
                    .collect();
                let target = if !live.contains(&current) {
                    nearest_live(sub.wanted, &live, rids)
                } else if current != sub.wanted && recovered(publisher, sub.wanted, now) {
                    Some(sub.wanted)
                } else {
                    None
                };
                if let Some(target) = target {
                    moves.push((viewer_id.clone(), owner_id.clone(), target, live));
                }
            }
        }
        for (viewer_id, owner_id, target, mut live) in moves {
            let Some(sub) = self
                .peers
                .get_mut(&viewer_id)
                .and_then(|viewer| viewer.subs.get_mut(&owner_id))
            else {
                continue;
            };
            if sub.fall_back(target) {
                live.sort_by_key(|rid| rid.rid());
                let live: Vec<String> = live.iter().map(ToString::to_string).collect();
                tracing::info!(
                    "SFU moving {viewer_id} from {owner_id}'s {} layer to {target} (wants {}; live: {})",
                    sub.layer.map(|layer| layer.to_string()).unwrap_or_default(),
                    sub.wanted,
                    live.join(",")
                );
                self.request_keyframe(&owner_id, Some(target));
            }
        }
    }

    // --- Negotiation ------------------------------------------------------------------------

    /// Offers the viewer's current set of boards, one negotiation at a time. Changes made
    /// while an offer is outstanding wait for its answer; the browser never offers again.
    fn negotiate(&mut self, id: &PeerId) {
        let Some(peer) = self.peers.get_mut(id) else {
            return;
        };
        if !(peer.ready && peer.negotiation.is_none() && peer.dirty) {
            return;
        }
        let mut api = peer.rtc.sdp_api();
        for mid in peer.to_stop.drain(..) {
            api.stop_media(mid);
        }
        let mut added = Vec::new();
        for (owner, sub) in &peer.subs {
            if sub.mid.is_none() {
                let mid = api.add_media(
                    MediaKind::Video,
                    Direction::SendOnly,
                    Some(owner.to_string()),
                    None,
                    None,
                );
                added.push((owner.clone(), mid));
            }
        }
        if peer.restart_ice {
            api.ice_restart(true);
        }
        let applied = api.apply();
        let restart = std::mem::take(&mut peer.restart_ice);
        peer.dirty = false;
        let Some((offer, pending)) = applied else {
            self.drive(id);
            return;
        };
        for (owner, mid) in &added {
            if let Some(sub) = peer.subs.get_mut(owner) {
                sub.mid = Some(*mid);
            }
            peer.sub_mids.insert(*mid, owner.clone());
        }
        let tracks: Map<String, Value> = peer
            .subs
            .iter()
            .filter_map(|(owner, sub)| {
                sub.mid
                    .map(|mid| (mid.to_string(), Value::String(owner.to_string())))
            })
            .collect();
        let sdp = simulcast_sdp::restore(&offer.to_sdp_string(), &peer.simulcast);
        let restart = if restart { " with an ICE restart" } else { "" };
        tracing::info!("SFU offered {} board(s) to {id}{restart}", tracks.len());
        let _ = peer
            .events
            .send(SfuEvent::Offer(json!({ "sdp": sdp, "tracks": tracks })));
        peer.negotiation = Some(Negotiation { pending, added });
        self.drive(id);
    }

    /// Once the browser has answered, each new subscription is switched to the codec its
    /// publisher actually sends, under whatever payload type this viewer assigned it. A
    /// browser that did not negotiate that codec gets nothing for that board.
    fn apply_codecs(&mut self, viewer_id: &PeerId) {
        let Some(viewer) = self.peers.get(viewer_id) else {
            return;
        };
        let mut applied = Vec::new();
        for (owner_id, sub) in &viewer.subs {
            if sub.codec().is_some() {
                continue;
            }
            let Some(mid) = sub.mid else {
                continue;
            };
            let Some(media) = viewer.rtc.media(mid) else {
                continue;
            };
            if media.disabled()
                || !matches!(media.direction(), Direction::SendOnly | Direction::SendRecv)
            {
                continue;
            }
            let Some(codec) = self
                .peers
                .get(owner_id)
                .and_then(|owner| owner.publisher.as_ref())
                .and_then(|publisher| publisher.codec)
            else {
                continue;
            };
            let config = viewer.rtc.codec_config();
            let candidates: Vec<CodecParams> = if media.remote_pts().is_empty() {
                config
                    .params()
                    .iter()
                    .filter_map(CodecParams::from_payload)
                    .collect()
            } else {
                media
                    .remote_pts()
                    .iter()
                    .filter_map(|pt| config.find(|params| params.pt() == *pt))
                    .filter_map(CodecParams::from_payload)
                    .collect()
            };
            if let Some(matched) = candidates
                .into_iter()
                .find(|candidate| candidate.same_bitstream(&codec))
            {
                applied.push((owner_id.clone(), matched));
            } else {
                tracing::warn!(
                    "SFU viewer {viewer_id} did not negotiate {}",
                    codec.codec.mime_type()
                );
            }
        }
        for (owner_id, codec) in applied {
            let Some(sub) = self
                .peers
                .get_mut(viewer_id)
                .and_then(|viewer| viewer.subs.get_mut(&owner_id))
            else {
                continue;
            };
            sub.set_codec(codec);
            let pending = sub.pending;
            self.request_keyframe(&owner_id, pending);
        }
    }

    // --- Forwarding -------------------------------------------------------------------------

    fn forward(&mut self, owner_id: &PeerId, rid: Encoding, packet: &RtpPacket) {
        let Some(codec) = self
            .peers
            .get(owner_id)
            .and_then(|owner| owner.publisher.as_ref())
            .and_then(|publisher| publisher.codec)
        else {
            return;
        };
        let keyframe = codec.keyframe(&packet.payload);
        let input = RtpIn {
            sequence_number: packet.header.sequence_number,
            timestamp: packet.header.timestamp,
            payload: &packet.payload,
            arrival: packet.timestamp,
        };
        let viewers: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(id, viewer)| *id != owner_id && viewer.subs.contains_key(owner_id))
            .map(|(id, _)| id.clone())
            .collect();
        for viewer_id in viewers {
            let Some(viewer) = self.peers.get_mut(&viewer_id) else {
                continue;
            };
            let Some(sub) = viewer.subs.get_mut(owner_id) else {
                continue;
            };
            match sub.route(rid, &input, keyframe) {
                Route::Forward(out) => {
                    if let (Some(mid), Some(codec)) = (sub.mid, sub.codec().copied()) {
                        let seq = sub.extend_sequence(out.sequence_number);
                        write(&mut viewer.rtc, mid, codec, seq, &out, packet);
                        self.drive(&viewer_id);
                    }
                }
                // The layer this viewer is waiting for is live but has not sent a keyframe.
                Route::Skip => {
                    if sub.ready() && sub.pending == Some(rid) {
                        self.request_keyframe(owner_id, Some(rid));
                    }
                }
            }
        }
    }

    fn request_keyframe(&mut self, owner_id: &PeerId, layer: Option<Encoding>) {
        let Some(layer) = layer else {
            return;
        };
        let now = Instant::now();
        let key = (owner_id.clone(), layer);
        if self
            .plis
            .get(&key)
            .is_some_and(|last| now.saturating_duration_since(*last) < PLI_INTERVAL)
        {
            return;
        }
        let Some(owner) = self.peers.get_mut(owner_id) else {
            return;
        };
        let Some(mid) = owner.publisher.as_ref().map(|publisher| publisher.mid) else {
            return;
        };
        let rid = layer.rid().map(Rid::from);
        let mut api = owner.rtc.direct_api();
        let Some(stream) = api.stream_rx_by_mid(mid, rid) else {
            return;
        };
        stream.request_keyframe(KeyframeRequestKind::Pli);
        self.plis.insert(key, now);
        self.drive(owner_id);
    }

    // --- Peers ------------------------------------------------------------------------------

    fn connection_failed(&mut self, id: &PeerId, now: Instant) {
        let report = self.describe_ice(id, now);
        let Some(peer) = self.peers.get_mut(id) else {
            return;
        };
        peer.ice_restarts
            .retain(|at| now.saturating_duration_since(*at) < ICE_RESTART_WINDOW);
        tracing::info!("SFU peer connection for {id} failed; ICE: {report}");
        if peer.ice_restarts.len() < MAX_ICE_RESTARTS && peer.ready {
            tracing::info!(
                "SFU restarting ICE for {id} ({}/{MAX_ICE_RESTARTS})",
                peer.ice_restarts.len() + 1
            );
            peer.ice_restarts.push(now);
            peer.restart_ice = true;
            peer.dirty = true;
            peer.ice.unconnected_since = Some(now);
            self.negotiate(id);
        } else {
            self.disconnect_peer(id, "failed");
        }
    }

    fn disconnect_peer(&mut self, id: &PeerId, reason: &str) {
        if let Some(peer) = self.peers.get(id) {
            let _ = peer.events.send(SfuEvent::Down(reason.to_owned()));
            self.remove_peer(id);
        }
    }

    /// The seat's connection and everyone's copy of its video go.
    fn remove_peer(&mut self, id: &PeerId) {
        let Some(mut peer) = self.peers.remove(id) else {
            return;
        };
        peer.rtc.disconnect();
        self.plis.retain(|(owner, _), _| owner != id);
        let viewers: Vec<PeerId> = self.peers.keys().cloned().collect();
        for viewer in viewers {
            self.unsubscribe(&viewer, id);
        }
    }

    /// The connection's ICE state as one log line, from what this room observed.
    fn describe_ice(&self, id: &PeerId, now: Instant) -> String {
        let Some(peer) = self.peers.get(id) else {
            return "stats unavailable".to_owned();
        };
        let millis = |at: Instant| {
            i64::try_from(at.saturating_duration_since(self.started).as_millis())
                .unwrap_or(i64::MAX)
        };
        let ice = &peer.ice;
        let ice_state = match ice.state {
            IceConnectionState::New => "new",
            IceConnectionState::Checking => "checking",
            IceConnectionState::Connected => "connected",
            IceConnectionState::Completed => "completed",
            IceConnectionState::Disconnected => "disconnected",
        };
        let mut entries = Vec::new();
        for (index, candidate) in ice.local.iter().enumerate() {
            entries.push(Entry::Local(candidate_stats(
                &format!("l{index}"),
                candidate.kind(),
                candidate.addr(),
            )));
        }
        for (index, (remote, (local, seen))) in ice.heard.iter().enumerate() {
            let local_id = ice
                .local
                .iter()
                .position(|candidate| candidate.local() == *local || candidate.addr() == *local)
                .map_or_else(|| "?".to_owned(), |index| format!("l{index}"));
            let kind = ice
                .remote
                .iter()
                .find(|candidate| candidate.addr() == *remote)
                .map_or(CandidateKind::PeerReflexive, Candidate::kind);
            entries.push(Entry::Remote(candidate_stats(
                &format!("r{index}"),
                kind,
                *remote,
            )));
            let nominated = ice.destination == Some(*remote);
            entries.push(Entry::Pair(PairStats {
                id: format!("p{index}"),
                local_candidate_id: local_id,
                remote_candidate_id: format!("r{index}"),
                priority: None,
                state: if nominated { "succeeded" } else { "heard" }.to_owned(),
                valid: nominated,
                nominated,
                last_seen: Some(millis(*seen)),
                requests_sent: 0,
                requests_received: 0,
                responses_received: 0,
                non_symmetric_responses_received: 0,
            }));
        }
        for (index, (name, port)) in ice.mdns.iter().enumerate() {
            entries.push(Entry::Remote(CandidateStats {
                id: format!("m{index}"),
                candidate_type: "host".to_owned(),
                address: Address::Name(name.clone()),
                port: *port,
            }));
            entries.push(Entry::Pair(PairStats {
                id: format!("pm{index}"),
                local_candidate_id: String::new(),
                remote_candidate_id: format!("m{index}"),
                priority: None,
                state: "unresolved".to_owned(),
                valid: false,
                nominated: false,
                last_seen: None,
                requests_sent: 0,
                requests_received: 0,
                responses_received: 0,
                non_symmetric_responses_received: 0,
            }));
        }
        let stats = IceStats {
            transport: Some(TransportStats {
                ice_role: Some("controlled".to_owned()),
                ice_state: Some(ice_state.to_owned()),
                dtls_state: Some(
                    if ice.dtls_connected {
                        "connected"
                    } else {
                        "new"
                    }
                    .to_owned(),
                ),
                packets_received: Some(ice.packets_received),
                packets_sent: Some(ice.packets_sent),
                selected_candidate_pair_changes: Some(ice.destination_changes),
                unmatched_requests: None,
            }),
            entries,
        };
        ice_report::format(&stats, millis(now))
    }
}

/// A new connection: RTP-level forwarding, video only, H.264 and VP8.
fn new_rtc(now: Instant) -> Rtc {
    let mut config = Rtc::builder().set_rtp_mode(true).clear_codecs();
    config.codec_config().add_h264(
        H264_PT.into(),
        Some(H264_RTX_PT.into()),
        true,
        H264_PROFILE_LEVEL_ID,
    );
    config.enable_vp8(true).build(now)
}

fn send(transport: &Transport, peer: &mut Peer, transmit: &Transmit) {
    peer.ice.packets_sent += 1;
    if peer.ice.destination != Some(transmit.destination) {
        if peer.ice.destination.is_some() {
            peer.ice.destination_changes += 1;
        }
        peer.ice.destination = Some(transmit.destination);
    }
    if let Some(allocation) = peer
        .turn
        .iter_mut()
        .find(|allocation| allocation.relayed() == Some(transmit.source))
    {
        allocation.send(transmit.destination, &transmit.contents, Instant::now());
        return;
    }
    if let Transport::Direct { sockets, .. } = transport {
        if let Some(socket) = sockets
            .iter()
            .find(|socket| socket.local == transmit.source)
        {
            socket.send(transmit.destination, &transmit.contents);
        } else {
            tracing::debug!("SFU has no socket for {}", transmit.source);
        }
    }
}

/// Writes one forwarded packet to the viewer's media section for the board.
fn write(rtc: &mut Rtc, mid: Mid, codec: CodecParams, seq: u64, out: &RtpOut, packet: &RtpPacket) {
    let mut api = rtc.direct_api();
    let Some(stream) = api.stream_tx_by_mid(mid, None) else {
        return;
    };
    let ext_vals = ExtensionValues {
        video_orientation: packet.header.ext_vals.video_orientation,
        ..ExtensionValues::default()
    };
    let mut rtp = RtpWrite::new(
        codec.pt,
        SeqNo::from(seq),
        out.timestamp,
        packet.timestamp,
        Arc::clone(&packet.payload),
    )
    .marker(packet.header.marker)
    .nackable(true)
    .ext_vals(ext_vals);
    if let Some(rewrite) = out.vp8 {
        let patch = Vp8Descriptor::parse(&packet.payload)
            .ok()
            .and_then(|descriptor| {
                let mut builder = descriptor.patch();
                if let Some(picture_id) = rewrite.picture_id {
                    builder = builder.picture_id(picture_id);
                }
                if let Some(tl0_pic_idx) = rewrite.tl0_pic_idx {
                    builder = builder.tl0_pic_idx(tl0_pic_idx);
                }
                if let Some(key_idx) = rewrite.key_idx {
                    builder = builder.key_idx(key_idx);
                }
                builder.build().ok()
            });
        if let Some(patch) = patch {
            rtp = rtp.vp8_patch(patch);
        }
    }
    stream.write_rtp(rtp);
}

/// Undoes the media sections a failed offer added, so the next offer adds them again.
fn revert(peer: &mut Peer, added: &[(PeerId, Mid)]) {
    for (owner, mid) in added {
        peer.sub_mids.remove(mid);
        if let Some(sub) = peer.subs.get_mut(owner).filter(|sub| sub.mid == Some(*mid)) {
            sub.mid = None;
        }
    }
}

/// Only the owner's chosen viewer sees a private reveal.
fn allowed(reveal_to: Option<&PeerId>, viewer: &PeerId) -> bool {
    reveal_to.is_none_or(|target| target == viewer)
}

/// When the layer last sent a packet, and since when it has been sending without a pause.
fn note_layer(publisher: &mut Publisher, rid: Encoding, now: Instant) {
    let since = match publisher.layers.get(&rid) {
        Some(seen) if now.saturating_duration_since(seen.last) < STALE => seen.since,
        _ => now,
    };
    publisher.layers.insert(rid, LayerSeen { last: now, since });
}

fn recovered(publisher: &Publisher, rid: Encoding, now: Instant) -> bool {
    publisher.layers.get(&rid).is_some_and(|seen| {
        now.saturating_duration_since(seen.last) < STALE
            && now.saturating_duration_since(seen.since) >= RECOVERED
    })
}

fn candidate_stats(id: &str, kind: CandidateKind, addr: SocketAddr) -> CandidateStats {
    let kind = match kind {
        CandidateKind::Host => "host",
        CandidateKind::PeerReflexive => "prflx",
        CandidateKind::ServerReflexive => "srflx",
        CandidateKind::Relayed => "relay",
    };
    CandidateStats {
        id: id.to_owned(),
        candidate_type: kind.to_owned(),
        address: Address::Ip(addr.ip()),
        port: addr.port(),
    }
}

/// A local candidate as the browser's `RTCIceCandidateInit` (`ICECandidate.to_json/1`).
fn candidate_json(candidate: &Candidate, peer: &Peer) -> Value {
    json!({
        "candidate": candidate.to_sdp_string(),
        "sdpMid": peer.first_mid,
        "sdpMLineIndex": 0,
        "usernameFragment": Value::Null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> (Room, mpsc::UnboundedReceiver<Command>) {
        let (commands, receiver) = mpsc::unbounded_channel();
        let (datagrams, _) = mpsc::channel(1);
        let transport = Transport::Direct {
            sockets: Vec::new(),
            candidates: Vec::new(),
        };
        (
            Room::new(
                RoomId::from("t"),
                transport,
                commands.downgrade(),
                datagrams,
            ),
            receiver,
        )
    }

    #[tokio::test]
    async fn a_failed_connection_without_an_answer_is_sent_down_with_its_ice_report() {
        let (mut room, _commands) = room();
        let (events, mut receiver) = mpsc::unbounded_channel();
        let id = PeerId::from("p");
        room.join(&id, false, events, None).unwrap();
        let mdns = json!({ "candidate": "candidate:1 1 udp 2122260223 abc.local 9 typ host" });
        assert_eq!(room.candidate(&id, &mdns), Ok(()));
        assert_eq!(room.candidate(&id, &json!({ "candidate": "" })), Ok(()));
        assert!(
            room.candidate(&id, &json!({ "candidate": "garbage" }))
                .is_err()
        );

        let report = room.describe_ice(&id, Instant::now());
        assert!(
            report.starts_with("controlled new, dtls new, rx 0pkt tx 0pkt"),
            "{report}"
        );
        assert!(
            report.contains("host abc.local:9 unresolved seen never"),
            "{report}"
        );

        room.connection_failed(&id, Instant::now());
        assert_eq!(receiver.try_recv(), Ok(SfuEvent::Down("failed".into())));
        assert!(room.peers.is_empty());
    }
}
