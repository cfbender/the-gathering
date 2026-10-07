//! A minimal TURN client (RFC 8656) over UDP, for relay-only operation.
//!
//! str0m does not speak TURN; it only needs relayed candidates and someone to carry their
//! datagrams. Each connection allocates a relayed address on every configured TURN server
//! (as `ex_ice` did for each peer connection), installs permissions for the browser's
//! candidate addresses, and exchanges media as Send and Data indications. Only `turn:` URLs
//! over UDP are supported; `turns:` and TCP transports are skipped.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket as StdUdpSocket};
use std::time::{Duration, Instant};

use str0m::Candidate;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use crate::net::{Datagram, ReadSocket};
use crate::stun::{
    self, ALLOCATE, CREATE_PERMISSION, Class, DATA, Message, REFRESH, SEND, TransactionId, attr,
    encode_xor_address,
};

const INITIAL_RTO: Duration = Duration::from_millis(500);
const MAX_ATTEMPTS: u32 = 6;
/// Permissions last five minutes; refreshed a minute early.
const PERMISSION_REFRESH: Duration = Duration::from_secs(240);
const DEFAULT_LIFETIME: u32 = 600;
const UDP: u8 = 17;
const FAR: Duration = Duration::from_secs(3_600);

/// A TURN server reachable over UDP, with its credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TurnServer {
    pub(crate) address: SocketAddr,
    pub(crate) username: String,
    pub(crate) credential: String,
}

/// The host and port of a `turn:` URL over UDP, or `None` for anything else.
pub(crate) fn udp_turn_host(url: &str) -> Option<(String, u16)> {
    let rest = url.strip_prefix("turn:")?;
    let (target, query) = rest.split_once('?').unwrap_or((rest, ""));
    let transport_ok = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .all(|(key, value)| key != "transport" || value.eq_ignore_ascii_case("udp"));
    if !transport_ok {
        return None;
    }
    let (host, port) = if let Some(bracketed) = target.strip_prefix('[') {
        let (host, after) = bracketed.split_once(']')?;
        let port = after
            .strip_prefix(':')
            .map_or(Some(3478), |port| port.parse().ok())?;
        (host.to_owned(), port)
    } else {
        match target.rsplit_once(':') {
            Some((host, port)) => (host.to_owned(), port.parse().ok()?),
            None => (target.to_owned(), 3478),
        }
    };
    (!host.is_empty()).then_some((host, port))
}

/// What a datagram from the TURN server amounted to.
#[derive(Debug)]
pub(crate) enum TurnInput {
    /// A datagram a peer sent to the relayed address.
    Relayed {
        relayed: SocketAddr,
        source: SocketAddr,
        data: Vec<u8>,
    },
    /// The allocation succeeded: a relayed candidate to add.
    Allocated(Candidate),
    Nothing,
}

#[derive(Clone, Debug)]
enum Request {
    Allocate,
    Refresh(u32),
    Permission(Vec<IpAddr>),
}

struct Transaction {
    id: TransactionId,
    request: Request,
    bytes: Vec<u8>,
    next_send: Instant,
    attempts: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Allocating,
    Allocated,
    Failed,
}

/// One relayed address on one TURN server, for one connection.
pub(crate) struct TurnAllocation {
    socket: ReadSocket,
    server: TurnServer,
    realm: Option<String>,
    nonce: Option<String>,
    key: Option<Vec<u8>>,
    state: State,
    relayed: Option<SocketAddr>,
    transactions: Vec<Transaction>,
    /// Peer addresses and when their permission was last requested.
    permissions: HashMap<IpAddr, Option<Instant>>,
    refresh_at: Option<Instant>,
}

impl TurnAllocation {
    /// Binds a local socket and asks `server` for an allocation.
    pub(crate) fn start(
        server: &TurnServer,
        datagrams: mpsc::Sender<Datagram>,
        now: Instant,
    ) -> io::Result<Self> {
        let any = if server.address.is_ipv4() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        };
        let socket = StdUdpSocket::bind(SocketAddr::new(any, 0))?;
        socket.set_nonblocking(true)?;
        let socket = ReadSocket::start(UdpSocket::from_std(socket)?, datagrams)?;
        let mut allocation = Self {
            socket,
            server: server.clone(),
            realm: None,
            nonce: None,
            key: None,
            state: State::Allocating,
            relayed: None,
            transactions: Vec::new(),
            permissions: HashMap::new(),
            refresh_at: None,
        };
        allocation.request(Request::Allocate, now);
        Ok(allocation)
    }

    /// The local socket the server talks to.
    pub(crate) fn socket_addr(&self) -> SocketAddr {
        self.socket.local
    }

    /// The relayed address, once allocated.
    pub(crate) fn relayed(&self) -> Option<SocketAddr> {
        self.relayed
    }

    pub(crate) fn next_timeout(&self) -> Instant {
        let far = Instant::now() + FAR;
        let mut next = self
            .transactions
            .iter()
            .map(|transaction| transaction.next_send)
            .min()
            .unwrap_or(far);
        if let Some(refresh) = self.refresh_at {
            next = next.min(refresh);
        }
        for requested in self.permissions.values().flatten() {
            next = next.min(*requested + PERMISSION_REFRESH);
        }
        next
    }

    /// Lets `peer` send to the relayed address (and installs the permission once allocated).
    pub(crate) fn permit(&mut self, peer: IpAddr, now: Instant) {
        if peer.is_ipv4() != self.server.address.is_ipv4() || self.permissions.contains_key(&peer) {
            return;
        }
        self.permissions.insert(peer, None);
        if self.state == State::Allocated {
            self.permissions.insert(peer, Some(now));
            self.request(Request::Permission(vec![peer]), now);
        }
    }

    /// Relays `data` to `peer` as a Send indication.
    pub(crate) fn send(&mut self, peer: SocketAddr, data: &[u8], now: Instant) {
        if self.state != State::Allocated {
            return;
        }
        self.permit(peer.ip(), now);
        let transaction = stun::new_transaction();
        let message = Message::new(SEND, Class::Indication, transaction)
            .with(
                attr::XOR_PEER_ADDRESS,
                encode_xor_address(peer, &transaction),
            )
            .with(attr::DATA, data.to_vec());
        self.socket.send(self.server.address, &message.encode(None));
    }

    /// Retransmits what is due and refreshes the allocation and its permissions.
    pub(crate) fn poll(&mut self, now: Instant) {
        if self.state == State::Failed {
            return;
        }
        let mut gave_up = false;
        for transaction in &mut self.transactions {
            if transaction.next_send > now {
                continue;
            }
            if transaction.attempts >= MAX_ATTEMPTS {
                gave_up = true;
                continue;
            }
            self.socket.send(self.server.address, &transaction.bytes);
            transaction.attempts += 1;
            transaction.next_send =
                now + INITIAL_RTO * 2u32.saturating_pow(transaction.attempts.saturating_sub(1));
        }
        if gave_up {
            tracing::warn!("SFU TURN server {} stopped answering", self.server.address);
            self.transactions
                .retain(|transaction| transaction.attempts < MAX_ATTEMPTS);
            if self.state == State::Allocating {
                self.state = State::Failed;
                return;
            }
        }
        if self.refresh_at.is_some_and(|at| at <= now) {
            self.refresh_at = None;
            self.request(Request::Refresh(DEFAULT_LIFETIME), now);
        }
        let stale: Vec<IpAddr> = self
            .permissions
            .iter()
            .filter(|(_, requested)| requested.is_some_and(|at| at + PERMISSION_REFRESH <= now))
            .map(|(peer, _)| *peer)
            .collect();
        if !stale.is_empty() {
            for peer in &stale {
                self.permissions.insert(*peer, Some(now));
            }
            self.request(Request::Permission(stale), now);
        }
    }

    /// Handles a datagram that arrived on the allocation's socket.
    pub(crate) fn handle(&mut self, source: SocketAddr, data: &[u8], now: Instant) -> TurnInput {
        if source != self.server.address {
            return TurnInput::Nothing;
        }
        let Some(message) = Message::decode(data) else {
            return TurnInput::Nothing;
        };
        if message.method == DATA && message.class == Class::Indication {
            let (Some(peer), Some(relayed), Some(payload)) = (
                message.address(attr::XOR_PEER_ADDRESS),
                self.relayed,
                message.attribute(attr::DATA),
            ) else {
                return TurnInput::Nothing;
            };
            return TurnInput::Relayed {
                relayed,
                source: peer,
                data: payload.to_vec(),
            };
        }
        let Some(index) = self
            .transactions
            .iter()
            .position(|transaction| transaction.id == message.transaction)
        else {
            return TurnInput::Nothing;
        };
        let transaction = self.transactions.swap_remove(index);
        match message.class {
            Class::Success => self.succeeded(&transaction.request, &message, now),
            Class::Error => {
                self.failed(transaction.request, &message, now);
                TurnInput::Nothing
            }
            Class::Request | Class::Indication => TurnInput::Nothing,
        }
    }

    fn succeeded(&mut self, request: &Request, message: &Message, now: Instant) -> TurnInput {
        let lifetime = message.lifetime().unwrap_or(DEFAULT_LIFETIME);
        match request {
            Request::Allocate => {
                let Some(relayed) = message.address(attr::XOR_RELAYED_ADDRESS) else {
                    self.state = State::Failed;
                    return TurnInput::Nothing;
                };
                self.state = State::Allocated;
                self.relayed = Some(relayed);
                self.schedule_refresh(lifetime, now);
                tracing::info!(
                    "SFU allocated relay address {relayed} on TURN server {}",
                    self.server.address
                );
                let waiting: Vec<IpAddr> = self.permissions.keys().copied().collect();
                if !waiting.is_empty() {
                    for peer in &waiting {
                        self.permissions.insert(*peer, Some(now));
                    }
                    self.request(Request::Permission(waiting), now);
                }
                match Candidate::relayed(relayed, self.socket.local, "udp") {
                    Ok(candidate) => TurnInput::Allocated(candidate),
                    Err(error) => {
                        tracing::warn!("SFU could not use relay address {relayed}: {error}");
                        TurnInput::Nothing
                    }
                }
            }
            Request::Refresh(_) => {
                self.schedule_refresh(lifetime, now);
                TurnInput::Nothing
            }
            Request::Permission(_) => TurnInput::Nothing,
        }
    }

    fn failed(&mut self, request: Request, message: &Message, now: Instant) {
        match message.error_code() {
            // Unauthenticated (the first request) or a stale nonce: retry with credentials.
            Some(401 | 438) if self.can_retry(message) => {
                if let Some(realm) = message.text(attr::REALM) {
                    self.key = Some(stun::long_term_key(
                        &self.server.username,
                        &realm,
                        &self.server.credential,
                    ));
                    self.realm = Some(realm);
                }
                self.nonce = message.text(attr::NONCE).or(self.nonce.take());
                self.request(request, now);
            }
            code => {
                tracing::warn!(
                    "SFU TURN server {} refused a request ({request:?}): {code:?}",
                    self.server.address
                );
                if matches!(request, Request::Allocate) {
                    self.state = State::Failed;
                }
            }
        }
    }

    /// A 401 is retried once per new nonce, so bad credentials do not loop.
    fn can_retry(&self, message: &Message) -> bool {
        let nonce = message.text(attr::NONCE);
        nonce.is_some() && (self.key.is_none() || nonce != self.nonce)
    }

    fn schedule_refresh(&mut self, lifetime: u32, now: Instant) {
        let lifetime = Duration::from_secs(u64::from(lifetime));
        self.refresh_at = Some(
            now + lifetime
                .saturating_sub(Duration::from_secs(60))
                .max(lifetime / 2),
        );
    }

    fn request(&mut self, request: Request, now: Instant) {
        let id = stun::new_transaction();
        let message = match &request {
            Request::Allocate => Message::new(ALLOCATE, Class::Request, id)
                .with(attr::REQUESTED_TRANSPORT, vec![UDP, 0, 0, 0])
                .with(attr::LIFETIME, DEFAULT_LIFETIME.to_be_bytes().to_vec()),
            Request::Refresh(lifetime) => Message::new(REFRESH, Class::Request, id)
                .with(attr::LIFETIME, lifetime.to_be_bytes().to_vec()),
            Request::Permission(peers) => peers.iter().fold(
                Message::new(CREATE_PERMISSION, Class::Request, id),
                |message, peer| {
                    message.with(
                        attr::XOR_PEER_ADDRESS,
                        encode_xor_address(SocketAddr::new(*peer, 0), &id),
                    )
                },
            ),
        };
        let bytes = self.authenticate(message);
        self.socket.send(self.server.address, &bytes);
        self.transactions.push(Transaction {
            id,
            request,
            bytes,
            next_send: now + INITIAL_RTO,
            attempts: 1,
        });
    }

    fn authenticate(&self, message: Message) -> Vec<u8> {
        match (&self.key, &self.realm, &self.nonce) {
            (Some(key), Some(realm), Some(nonce)) => message
                .with(attr::USERNAME, self.server.username.as_bytes().to_vec())
                .with(attr::REALM, realm.as_bytes().to_vec())
                .with(attr::NONCE, nonce.as_bytes().to_vec())
                .encode(Some(key)),
            _ => message.encode(None),
        }
    }
}

impl Drop for TurnAllocation {
    /// Frees the allocation on the server rather than letting it time out.
    fn drop(&mut self) {
        if self.state == State::Allocated {
            let message = Message::new(REFRESH, Class::Request, stun::new_transaction())
                .with(attr::LIFETIME, vec![0; 4]);
            let bytes = self.authenticate(message);
            self.socket.send(self.server.address, &bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_udp_turn_urls_are_used() {
        assert_eq!(
            udp_turn_host("turn:turn.cloudflare.com:3478?transport=udp"),
            Some(("turn.cloudflare.com".into(), 3478))
        );
        assert_eq!(
            udp_turn_host("turn:turn.example.com"),
            Some(("turn.example.com".into(), 3478))
        );
        assert_eq!(
            udp_turn_host("turn:[2001:db8::1]:3479"),
            Some(("2001:db8::1".into(), 3479))
        );
        assert_eq!(
            udp_turn_host("turn:turn.cloudflare.com:3478?transport=tcp"),
            None
        );
        assert_eq!(
            udp_turn_host("turns:turn.cloudflare.com:5349?transport=tcp"),
            None
        );
        assert_eq!(udp_turn_host("stun:stun.cloudflare.com:3478"), None);
    }
}
