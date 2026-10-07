//! Two in-process WebRTC clients (str0m, over loopback UDP) talking to the SFU: a publisher
//! sending simulcast VP8 and a viewer receiving the layer it asked for.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Client, Harness, settings};
use serde_json::json;
use the_gathering_sfu::{Sfu, SfuError, SfuEvent};

const ROOM: &str = "table-1";
const A: &str = "a";
const B: &str = "b";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forwards_the_chosen_layer_switches_layers_and_renegotiates_when_a_seat_leaves() {
    tokio::time::timeout(Duration::from_secs(60), Box::pin(scenario()))
        .await
        .expect("the scenario finished in time");
}

async fn scenario() {
    let sfu =
        Sfu::with_host_addresses(settings(53_000, 53_099), vec!["127.0.0.1".parse().unwrap()]);
    assert_eq!(sfu.client_info(), json!({ "transport": "direct" }));

    // Nothing works before joining.
    assert_eq!(sfu.offer(ROOM, A, "v=0").await, Err(SfuError::NotJoined));
    assert_eq!(
        sfu.layer(ROOM, B, A, "x").await,
        Err(SfuError::Rejected("invalid layer".into()))
    );

    let publisher = Box::pin(Client::publisher(&sfu, ROOM, A)).await;
    let viewer = Box::pin(Client::viewer(&sfu, ROOM, B)).await;
    let mut harness = Harness {
        sfu: &sfu,
        room: ROOM,
        a: publisher,
        b: viewer,
    };

    // The viewer's candidate trickles in over the API too.
    let candidate = harness.b.candidate_json();
    assert_eq!(sfu.candidate(ROOM, B, &candidate).await, Ok(()));
    assert_eq!(
        sfu.candidate(ROOM, B, &json!({ "sdpMid": "0" })).await,
        Err(SfuError::Rejected("invalid candidate".into()))
    );

    // The viewer is offered the publisher's board and gets the medium layer.
    harness
        .run_until("the viewer sees medium", |h| {
            h.b.received_after_first(0, b'm') >= 20
        })
        .await;
    let offer = harness.b.offers.first().expect("an offer").clone();
    let tracks = offer["tracks"].as_object().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks.values().next().unwrap(), A);
    assert!(offer["sdp"].as_str().unwrap().contains("a=sendonly"));
    harness.b.assert_only_after_first(0, b'm');
    harness.b.assert_continuous();

    // A layer switch: the high layer arrives after a keyframe request, numbered on.
    let switched_at = harness.b.received.len();
    assert_eq!(sfu.layer(ROOM, B, A, "h").await, Ok(()));
    harness
        .run_until("the viewer sees high", |h| {
            h.b.received_after_first(switched_at, b'h') >= 20
        })
        .await;
    harness.b.assert_only_after_first(switched_at, b'h');
    harness.b.assert_continuous();
    assert!(
        harness.a.keyframe_requests > 0,
        "the publisher was asked for keyframes"
    );

    // Messages for another seat are relayed verbatim.
    assert_eq!(
        sfu.relay(ROOM, A, B, json!({ "type": "crop" })).await,
        Ok(())
    );
    harness
        .run_until("the message arrives", |h| !h.b.messages.is_empty())
        .await;
    assert_eq!(
        harness.b.messages[0],
        json!({ "from": A, "message": { "type": "crop" } })
    );
    assert_eq!(
        sfu.relay(ROOM, A, "nobody", json!({})).await,
        Err(SfuError::NotJoined)
    );

    // A private reveal to someone else hides the board; ending it brings it back.
    assert_eq!(sfu.reveal(ROOM, A, Some("someone-else")).await, Ok(()));
    harness.settle(Duration::from_millis(200)).await;
    let hidden_at = harness.b.received.len();
    harness.settle(Duration::from_millis(400)).await;
    assert_eq!(
        harness.b.received.len(),
        hidden_at,
        "nothing arrives while hidden"
    );
    assert_eq!(sfu.reveal(ROOM, A, None).await, Ok(()));
    harness
        .run_until("the board returns", |h| h.b.received.len() > hidden_at + 10)
        .await;
    harness.b.assert_continuous();

    // The publisher leaves: the viewer is offered a set without its board.
    let offers = harness.b.offers.len();
    sfu.leave(ROOM, A).await;
    harness
        .run_until("the viewer is re-offered", |h| h.b.offers.len() > offers)
        .await;
    let offer = harness.b.offers.last().unwrap();
    assert_eq!(offer["tracks"], json!({}));
    assert!(
        offer["sdp"].as_str().unwrap().contains("m=video 0 "),
        "the board's section is stopped"
    );
    assert_eq!(sfu.layer(ROOM, B, A, "m").await, Err(SfuError::NotJoined));

    // The last seat leaving stops the room.
    drop(harness.b.events.take());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(sfu.answer(ROOM, B, "v=0").await, Err(SfuError::NotJoined));
}

#[tokio::test]
async fn a_rejoin_replaces_the_connection_and_a_dropped_channel_counts_as_leaving() {
    let sfu =
        Sfu::with_host_addresses(settings(53_100, 53_199), vec!["127.0.0.1".parse().unwrap()]);
    let (first, mut first_events) = tokio::sync::mpsc::unbounded_channel();
    assert_eq!(sfu.join(ROOM, A, false, first).await, Ok(()));
    let (second, second_events) = tokio::sync::mpsc::unbounded_channel();
    assert_eq!(sfu.join(ROOM, A, false, second).await, Ok(()));
    // The replaced connection is not told to go down; its channel is the one rejoining.
    assert!(first_events.try_recv().is_err());

    assert_eq!(
        sfu.answer(ROOM, A, "v=0").await,
        Err(SfuError::Rejected("unexpected answer".into()))
    );
    let rejected = sfu.offer(ROOM, A, "not sdp").await;
    assert!(
        matches!(rejected, Err(SfuError::Rejected(_))),
        "{rejected:?}"
    );

    drop(second_events);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(sfu.offer(ROOM, A, "v=0").await, Err(SfuError::NotJoined));
}

async fn next_offer(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<SfuEvent>,
) -> serde_json::Value {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("an event in time");
        match event.expect("the channel is open") {
            SfuEvent::Offer(payload) => return payload,
            SfuEvent::Down(reason) => panic!("went down: {reason}"),
            _ => {}
        }
    }
}

/// The Elixir room waited forever after a rejected answer; this one offers again, and tells a
/// browser that keeps failing to reconnect.
#[tokio::test]
async fn a_rejected_answer_is_offered_again_and_repeated_failures_reconnect() {
    let sfu =
        Sfu::with_host_addresses(settings(53_300, 53_399), vec!["127.0.0.1".parse().unwrap()]);
    let _publisher = Box::pin(Client::publisher(&sfu, ROOM, A)).await;
    let mut viewer = Box::pin(Client::viewer(&sfu, ROOM, B)).await;
    let mut events = viewer.events.take().unwrap();

    let first = next_offer(&mut events).await;
    assert_eq!(
        first["tracks"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap(),
        A
    );
    assert!(matches!(
        sfu.answer(ROOM, B, "garbage").await,
        Err(SfuError::Rejected(_))
    ));

    let second = next_offer(&mut events).await;
    assert_eq!(
        second["tracks"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap(),
        A
    );
    let offer = str0m::change::SdpOffer::from_sdp_string(second["sdp"].as_str().unwrap()).unwrap();
    let answer = viewer.rtc.sdp_api().accept_offer(offer).unwrap();
    assert_eq!(sfu.answer(ROOM, B, &answer.to_sdp_string()).await, Ok(()));
    assert_eq!(
        sfu.answer(ROOM, B, &answer.to_sdp_string()).await,
        Err(SfuError::Rejected("unexpected answer".into())),
        "nothing is outstanding any more"
    );

    // A seat that keeps sending answers that cannot be applied is told to reconnect. (A
    // rejected offer that only stopped boards is not repeated: str0m stops media locally at
    // once, and the next offer carries the stopped section.)
    sfu.leave(ROOM, A).await;
    let stop = next_offer(&mut events).await;
    assert_eq!(stop["tracks"], serde_json::json!({}));
    let offer = str0m::change::SdpOffer::from_sdp_string(stop["sdp"].as_str().unwrap()).unwrap();
    let answer = viewer.rtc.sdp_api().accept_offer(offer).unwrap();
    assert_eq!(sfu.answer(ROOM, B, &answer.to_sdp_string()).await, Ok(()));
    let _second_publisher = Box::pin(Client::publisher(&sfu, ROOM, "c")).await;
    for _ in 0..3 {
        let offer = next_offer(&mut events).await;
        assert_eq!(
            offer["tracks"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap(),
            "c"
        );
        assert!(sfu.answer(ROOM, B, "garbage").await.is_err());
    }
    let down = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap();
    assert_eq!(down, Some(SfuEvent::Down("negotiation failed".into())));
}

/// An LXC container whose service started before DHCP: the first room finds only loopback.
/// Once the interface has its address, the next room binds and announces it without a
/// restart, and media flows over it. (The address source stands in for the interface list;
/// 127.0.0.2 stands in for the LAN address.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_room_gathers_the_host_addresses_present_when_it_starts() {
    let addresses: Arc<Mutex<Vec<IpAddr>>> =
        Arc::new(Mutex::new(vec!["127.0.0.1".parse().unwrap()]));
    let source = Arc::clone(&addresses);
    let sfu = Sfu::with_host_source(
        settings(53_400, 53_499),
        Arc::new(move || source.lock().unwrap().clone()),
    );
    let host = |answer: &str, ip: &str| answer.contains(&format!(" {ip} 53400 typ host"));

    let early = Box::pin(Client::viewer(&sfu, "early", A)).await;
    assert!(host(&early.answer, "127.0.0.1"), "{}", early.answer);

    *addresses.lock().unwrap() = vec!["127.0.0.2".parse().unwrap()];

    // A running room keeps the sockets it started with.
    let early_second = Box::pin(Client::viewer(&sfu, "early", B)).await;
    assert!(
        host(&early_second.answer, "127.0.0.1"),
        "{}",
        early_second.answer
    );

    // The next room uses the address that appeared, and connects on it.
    let publisher = Box::pin(Client::publisher(&sfu, "late", A)).await;
    let viewer = Box::pin(Client::viewer(&sfu, "late", B)).await;
    for answer in [&publisher.answer, &viewer.answer] {
        assert!(host(answer, "127.0.0.2"), "{answer}");
        assert!(!answer.contains(" 127.0.0.1 "), "{answer}");
    }
    let mut harness = Harness {
        sfu: &sfu,
        room: "late",
        a: publisher,
        b: viewer,
    };
    tokio::time::timeout(
        Duration::from_secs(30),
        harness.run_until("the viewer sees medium over 127.0.0.2", |h| {
            h.b.received_after_first(0, b'm') >= 10
        }),
    )
    .await
    .expect("media flowed in time");
}
