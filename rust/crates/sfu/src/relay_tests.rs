//! Relay-only operation end to end: the SFU allocates on a fake TURN server and two str0m
//! clients reach it only through the relayed address.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::net::UdpSocket;

use crate::common::{Client, Harness, settings};
use crate::stun::{
    self, ALLOCATE, CREATE_PERMISSION, Class, DATA, Message, REFRESH, SEND, attr,
    encode_xor_address,
};
use crate::{IceServer, Sfu};

const USER: &str = "user";
const PASSWORD: &str = "secret";
const REALM: &str = "test";

/// What the fake server saw, for assertions.
#[derive(Default)]
struct Seen {
    unauthenticated: AtomicUsize,
    allocations: AtomicUsize,
    permissions: AtomicUsize,
    relayed_out: AtomicUsize,
    relayed_in: AtomicUsize,
}

/// A TURN server enforcing long-term credentials and permissions, one relay socket per
/// allocation.
async fn fake_turn_server(seen: Arc<Seen>) -> SocketAddr {
    struct Allocation {
        relayed: SocketAddr,
        relay: Arc<UdpSocket>,
        permitted: Arc<Mutex<HashSet<IpAddr>>>,
    }

    let control = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let address = control.local_addr().unwrap();
    let key = stun::long_term_key(USER, REALM, PASSWORD);
    tokio::spawn(async move {
        let mut allocations: HashMap<SocketAddr, Allocation> = HashMap::new();
        let mut buffer = vec![0; 2_048];
        loop {
            let (length, source) = control.recv_from(&mut buffer).await.unwrap();
            let data = &buffer[..length];
            let Some(message) = Message::decode(data) else {
                continue;
            };
            if message.class == Class::Indication {
                let Some(allocation) = allocations.get(&source).filter(|_| message.method == SEND)
                else {
                    continue;
                };
                let peer = message.address(attr::XOR_PEER_ADDRESS).unwrap();
                if allocation.permitted.lock().unwrap().contains(&peer.ip()) {
                    seen.relayed_out.fetch_add(1, Ordering::Relaxed);
                    allocation
                        .relay
                        .send_to(message.attribute(attr::DATA).unwrap(), peer)
                        .await
                        .unwrap();
                }
                continue;
            }
            if !stun::verify_integrity(data, &key) {
                seen.unauthenticated.fetch_add(1, Ordering::Relaxed);
                let challenge = Message::new(message.method, Class::Error, message.transaction)
                    .with(attr::ERROR_CODE, vec![0, 0, 4, 1])
                    .with(attr::REALM, REALM.as_bytes().to_vec())
                    .with(attr::NONCE, b"nonce-1".to_vec());
                control
                    .send_to(&challenge.encode(None), source)
                    .await
                    .unwrap();
                continue;
            }
            let reply = Message::new(message.method, Class::Success, message.transaction);
            let reply = match message.method {
                ALLOCATE => {
                    seen.allocations.fetch_add(1, Ordering::Relaxed);
                    let relay = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
                    let relayed = relay.local_addr().unwrap();
                    let permitted = Arc::new(Mutex::new(HashSet::new()));
                    tokio::spawn(relay_reader(
                        Arc::clone(&relay),
                        Arc::clone(&permitted),
                        Arc::clone(&control),
                        source,
                        Arc::clone(&seen),
                    ));
                    allocations.insert(
                        source,
                        Allocation {
                            relayed,
                            relay,
                            permitted,
                        },
                    );
                    reply
                        .with(
                            attr::XOR_RELAYED_ADDRESS,
                            encode_xor_address(relayed, &message.transaction),
                        )
                        .with(attr::LIFETIME, 600u32.to_be_bytes().to_vec())
                }
                CREATE_PERMISSION => {
                    let Some(allocation) = allocations.get(&source) else {
                        continue;
                    };
                    for (_, value) in message
                        .attributes
                        .iter()
                        .filter(|(kind, _)| *kind == attr::XOR_PEER_ADDRESS)
                    {
                        let single = Message::new(DATA, Class::Indication, message.transaction)
                            .with(attr::XOR_PEER_ADDRESS, value.clone());
                        let peer = single.address(attr::XOR_PEER_ADDRESS).unwrap();
                        allocation.permitted.lock().unwrap().insert(peer.ip());
                    }
                    let _ = allocation.relayed;
                    seen.permissions.fetch_add(1, Ordering::Relaxed);
                    reply
                }
                REFRESH => reply.with(attr::LIFETIME, 600u32.to_be_bytes().to_vec()),
                _ => continue,
            };
            control
                .send_to(&reply.encode(Some(&key)), source)
                .await
                .unwrap();
        }
    });
    address
}

/// Wraps what peers send to an allocation's relayed address in Data indications.
async fn relay_reader(
    relay: Arc<UdpSocket>,
    permitted: Arc<Mutex<HashSet<IpAddr>>>,
    control: Arc<UdpSocket>,
    client: SocketAddr,
    seen: Arc<Seen>,
) {
    let mut buffer = vec![0; 2_048];
    loop {
        let (length, peer) = relay.recv_from(&mut buffer).await.unwrap();
        if !permitted.lock().unwrap().contains(&peer.ip()) {
            continue;
        }
        seen.relayed_in.fetch_add(1, Ordering::Relaxed);
        let transaction = stun::new_transaction();
        let data = Message::new(DATA, Class::Indication, transaction)
            .with(
                attr::XOR_PEER_ADDRESS,
                encode_xor_address(peer, &transaction),
            )
            .with(attr::DATA, buffer[..length].to_vec());
        control.send_to(&data.encode(None), client).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relay_only_media_flows_through_the_turn_server() {
    tokio::time::timeout(
        Duration::from_secs(60),
        Box::pin(async {
            let seen = Arc::new(Seen::default());
            let server = fake_turn_server(Arc::clone(&seen)).await;
            let mut config = settings(53_200, 53_299);
            config.relay = Some(Arc::new(move || {
                Box::pin(async move {
                    vec![
                        // Unusable entries are skipped.
                        IceServer {
                            urls: vec!["stun:127.0.0.1:1".into()],
                            username: None,
                            credential: None,
                        },
                        IceServer {
                            urls: vec![
                                format!("turns:127.0.0.1:{}?transport=tcp", server.port()),
                                format!("turn:{server}?transport=udp"),
                            ],
                            username: Some(USER.into()),
                            credential: Some(PASSWORD.into()),
                        },
                    ]
                })
            }));
            let sfu = Sfu::with_host_addresses(config, vec!["127.0.0.1".parse().unwrap()]);
            assert_eq!(sfu.client_info(), json!({ "transport": "relay" }));

            let publisher = Box::pin(Client::publisher(&sfu, "relay-table", "a")).await;
            let viewer = Box::pin(Client::viewer(&sfu, "relay-table", "b")).await;
            let mut harness = Harness {
                sfu: &sfu,
                room: "relay-table",
                a: publisher,
                b: viewer,
            };
            harness
                .run_until("the viewer sees medium through the relay", |h| {
                    h.b.received_after_first(0, b'm') >= 10
                })
                .await;
            harness.b.assert_only_after_first(0, b'm');
            harness.b.assert_continuous();

            assert_eq!(
                seen.allocations.load(Ordering::Relaxed),
                2,
                "one allocation per connection"
            );
            assert!(
                seen.unauthenticated.load(Ordering::Relaxed) >= 2,
                "each was challenged first"
            );
            assert!(seen.permissions.load(Ordering::Relaxed) >= 2);
            assert!(
                seen.relayed_in.load(Ordering::Relaxed) > 0
                    && seen.relayed_out.load(Ordering::Relaxed) > 0
            );
        }),
    )
    .await
    .expect("the scenario finished in time");
}
