//! A str0m WebRTC client driven over loopback UDP, standing in for a browser.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use str0m::change::{SdpAnswer, SdpOffer};
use str0m::format::Codec;
use str0m::media::{Direction, MediaKind, Mid, Pt, Rid, Simulcast, SimulcastLayer};
use str0m::net::{Protocol, Receive};
use str0m::rtp::{RtpWrite, SeqNo};
use str0m::{Candidate, Event, Input, Output, Rtc};
use the_gathering_sfu::{Settings, Sfu, SfuEvent};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

pub fn settings(port_min: u16, port_max: u16) -> Settings {
    Settings {
        port_min,
        port_max,
        public_ip: None,
        ipv6: false,
        relay: None,
    }
}

const FRAME: Duration = Duration::from_millis(33);
const LAYERS: [&str; 3] = ["l", "m", "h"];

/// The publisher's camera: one single-packet VP8 frame per layer per tick. Keyframes are sent
/// on each layer's first frame and whenever the SFU asks.
pub struct Camera {
    mid: Mid,
    pt: Pt,
    next_frame: Instant,
    frame: u32,
    keyframe_due: HashSet<&'static str>,
    seq: [u64; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct Received {
    pub layer: u8,
    pub seq: u64,
    pub keyframe: bool,
}

pub struct Client {
    pub peer: String,
    pub rtc: Rtc,
    socket: UdpSocket,
    local: SocketAddr,
    pub events: Option<mpsc::UnboundedReceiver<SfuEvent>>,
    camera: Option<Camera>,
    pub received: Vec<Received>,
    pub offers: Vec<Value>,
    pub messages: Vec<Value>,
    pub keyframe_requests: usize,
    requested: HashSet<&'static str>,
    next_timeout: Instant,
}

impl Client {
    async fn new(sfu: &Sfu, room: &str, peer: &str, spectator: bool) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let local = socket.local_addr().unwrap();
        let mut rtc = Rtc::builder()
            .set_rtp_mode(true)
            .clear_codecs()
            .enable_vp8(true)
            .build(Instant::now());
        rtc.add_local_candidate(Candidate::host(local, "udp").unwrap());
        let (events, receiver) = mpsc::unbounded_channel();
        sfu.join(room, peer, spectator, events).await.unwrap();
        Self {
            peer: peer.to_owned(),
            rtc,
            socket,
            local,
            events: Some(receiver),
            camera: None,
            received: Vec::new(),
            offers: Vec::new(),
            messages: Vec::new(),
            keyframe_requests: 0,
            requested: HashSet::new(),
            next_timeout: Instant::now(),
        }
    }

    /// A seat publishing its camera as `l`, `m`, `h`, offered the way the browser does.
    pub async fn publisher(sfu: &Sfu, room: &str, peer: &str) -> Self {
        let mut client = Self::new(sfu, room, peer, false).await;
        let mut simulcast = Simulcast::new();
        for rid in LAYERS {
            simulcast.add_send_layer(SimulcastLayer::new(rid));
        }
        let mut change = client.rtc.sdp_api();
        let mid = change.add_media(
            MediaKind::Video,
            Direction::SendOnly,
            Some("camera".into()),
            None,
            Some(simulcast),
        );
        let (offer, pending) = change.apply().unwrap();
        let offer = offer.to_sdp_string();
        assert!(offer.contains("a=simulcast:send l;m;h"), "{offer}");
        let answer = sfu.offer(room, peer, &offer).await.unwrap();
        assert!(answer.contains("a=simulcast:recv l;m;h"), "{answer}");
        client
            .rtc
            .sdp_api()
            .accept_answer(pending, SdpAnswer::from_sdp_string(&answer).unwrap())
            .unwrap();
        let pt = client
            .rtc
            .codec_config()
            .find(|params| params.spec().codec == Codec::Vp8)
            .unwrap()
            .pt();
        client.camera = Some(Camera {
            mid,
            pt,
            next_frame: Instant::now(),
            frame: 0,
            keyframe_due: LAYERS.into_iter().collect(),
            seq: [1_000, 20_000, 40_000],
        });
        client
    }

    /// A spectator: one receive-only section for the transport, then whatever the SFU offers.
    pub async fn viewer(sfu: &Sfu, room: &str, peer: &str) -> Self {
        let mut client = Self::new(sfu, room, peer, true).await;
        let mut change = client.rtc.sdp_api();
        change.add_media(MediaKind::Video, Direction::RecvOnly, None, None, None);
        let (offer, pending) = change.apply().unwrap();
        let answer = sfu.offer(room, peer, &offer.to_sdp_string()).await.unwrap();
        client
            .rtc
            .sdp_api()
            .accept_answer(pending, SdpAnswer::from_sdp_string(&answer).unwrap())
            .unwrap();
        client
    }

    pub fn candidate_json(&self) -> Value {
        let candidate = Candidate::host(self.local, "udp").unwrap();
        json!({ "candidate": candidate.to_sdp_string(), "sdpMid": "0", "sdpMLineIndex": 0 })
    }

    /// Packets received from the first one of `layer` at or after index `since` on.
    pub fn received_after_first(&self, since: usize, layer: u8) -> usize {
        self.received
            .iter()
            .skip(since)
            .skip_while(|packet| packet.layer != layer)
            .count()
    }

    /// From the first packet of `layer` at or after index `since`, every packet is of `layer`,
    /// and that first one is a keyframe.
    pub fn assert_only_after_first(&self, since: usize, layer: u8) {
        let mut after = self
            .received
            .iter()
            .skip(since)
            .skip_while(|packet| packet.layer != layer)
            .peekable();
        assert!(
            after.peek().unwrap().keyframe,
            "a switch starts on a keyframe"
        );
        let layers: Vec<u8> = after.map(|packet| packet.layer).collect();
        assert!(
            layers.iter().all(|got| *got == layer),
            "only {} after the switch: {layers:?}",
            char::from(layer)
        );
    }

    /// The viewer saw one stream: every packet numbered one past the one before.
    pub fn assert_continuous(&self) {
        for pair in self.received.windows(2) {
            assert_eq!(
                pair[1].seq,
                pair[0].seq + 1,
                "continuous sequence numbers: {:?}",
                self.received
            );
        }
    }

    fn drain(&mut self) {
        loop {
            match self.rtc.poll_output().unwrap() {
                Output::Timeout(at) => {
                    self.next_timeout = at;
                    return;
                }
                Output::Transmit(transmit) => {
                    let _ = self
                        .socket
                        .try_send_to(&transmit.contents, transmit.destination);
                }
                Output::Event(event) => self.on_rtc_event(event),
            }
        }
    }

    fn on_rtc_event(&mut self, event: Event) {
        match event {
            Event::RtpPacket(packet) => {
                let payload = &packet.payload;
                self.received.push(Received {
                    layer: payload.get(2).copied().unwrap_or(0),
                    seq: *packet.seq_no,
                    keyframe: payload.get(1) == Some(&0),
                });
            }
            Event::KeyframeRequest(request) => {
                self.keyframe_requests += 1;
                match request.rid {
                    Some(rid) => {
                        if let Some(layer) = LAYERS.into_iter().find(|layer| *layer == &*rid) {
                            self.requested.insert(layer);
                        }
                    }
                    None => self.requested.extend(LAYERS),
                }
            }
            _ => {}
        }
    }

    fn receive(&mut self, data: &[u8], source: SocketAddr) {
        let Ok(contents) = data.try_into() else {
            return;
        };
        let input = Input::Receive(
            Instant::now(),
            Receive {
                proto: Protocol::Udp,
                source,
                destination: self.local,
                contents,
            },
        );
        self.rtc.handle_input(input).unwrap();
        self.drain();
    }

    fn tick(&mut self, now: Instant) {
        self.rtc.handle_input(Input::Timeout(now)).unwrap();
        self.drain();
        let Some(mut camera) = self.camera.take() else {
            return;
        };
        if now < camera.next_frame || !self.rtc.is_connected() {
            self.camera = Some(camera);
            return;
        }
        camera.keyframe_due.extend(self.requested.drain());
        camera.next_frame = now + FRAME;
        camera.frame += 1;
        for (index, layer) in LAYERS.into_iter().enumerate() {
            let keyframe = camera.keyframe_due.remove(layer);
            // VP8 descriptor (S=1, partition 0), the payload header's P bit, then a marker.
            let payload = vec![
                0x10,
                u8::from(!keyframe),
                layer.as_bytes()[0],
                u8::try_from(camera.frame % 256).unwrap(),
            ];
            camera.seq[index] += 1;
            let time = camera.frame * 3_000;
            let mut api = self.rtc.direct_api();
            let stream = api
                .stream_tx_by_mid(camera.mid, Some(Rid::from(layer)))
                .expect("a stream per layer");
            stream.write_rtp(
                RtpWrite::new(
                    camera.pt,
                    SeqNo::from(camera.seq[index]),
                    time,
                    now,
                    payload,
                )
                .marker(true),
            );
            self.drain();
        }
        self.camera = Some(camera);
    }

    async fn on_sfu_event(&mut self, event: SfuEvent, sfu: &Sfu, room: &str) {
        match event {
            SfuEvent::Offer(payload) => {
                let offer = SdpOffer::from_sdp_string(payload["sdp"].as_str().unwrap()).unwrap();
                let answer = self.rtc.sdp_api().accept_offer(offer).unwrap();
                self.offers.push(payload);
                sfu.answer(room, &self.peer, &answer.to_sdp_string())
                    .await
                    .unwrap();
                self.drain();
            }
            SfuEvent::PeerMessage(payload) => self.messages.push(payload),
            SfuEvent::Candidate(payload) => {
                let line = payload["candidate"]["candidate"].as_str().unwrap();
                self.rtc
                    .add_remote_candidate(Candidate::from_sdp_string(line).unwrap());
                self.drain();
            }
            SfuEvent::Down(reason) => panic!("{} went down: {reason}", self.peer),
        }
    }
}

/// Drives both clients (the SFU runs on its own tasks).
pub struct Harness<'a> {
    pub sfu: &'a Sfu,
    pub room: &'a str,
    pub a: Client,
    pub b: Client,
}

impl Harness<'_> {
    pub async fn run_until(&mut self, what: &str, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out waiting until {what}");
            self.step().await;
        }
    }

    pub async fn settle(&mut self, duration: Duration) {
        let until = Instant::now() + duration;
        while Instant::now() < until {
            self.step().await;
        }
    }

    async fn step(&mut self) {
        let mut buffer_a = vec![0; 2_048];
        let mut buffer_b = vec![0; 2_048];
        let now = Instant::now();
        let mut wake = self
            .a
            .next_timeout
            .min(self.b.next_timeout)
            .min(now + Duration::from_millis(20));
        if let Some(camera) = &self.a.camera {
            wake = wake.min(camera.next_frame);
        }
        let wake = tokio::time::Instant::from_std(wake.max(now));
        let (sfu, room) = (self.sfu, self.room);
        let (a, b) = (&mut self.a, &mut self.b);
        tokio::select! {
            received = a.socket.recv_from(&mut buffer_a) => {
                let (length, source) = received.unwrap();
                a.receive(&buffer_a[..length], source);
            }
            received = b.socket.recv_from(&mut buffer_b) => {
                let (length, source) = received.unwrap();
                b.receive(&buffer_b[..length], source);
            }
            Some(event) = next_event(&mut a.events) => a.on_sfu_event(event, sfu, room).await,
            Some(event) = next_event(&mut b.events) => b.on_sfu_event(event, sfu, room).await,
            () = tokio::time::sleep_until(wake) => {
                let now = Instant::now();
                a.tick(now);
                b.tick(now);
            }
        }
    }
}

async fn next_event(events: &mut Option<mpsc::UnboundedReceiver<SfuEvent>>) -> Option<SfuEvent> {
    let Some(receiver) = events else {
        return std::future::pending().await;
    };
    let event = receiver.recv().await;
    if event.is_none() {
        *events = None;
    }
    event
}
