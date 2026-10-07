//! The SFU answers the offers real browsers make (captured shapes of Chrome's and Firefox's
//! simulcast camera offers) with SDP they accept.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::time::Duration;

use the_gathering_sfu::{Settings, Sfu, SfuEvent};
use tokio::sync::mpsc;

const CHROME_OFFER: &str = "v=0\r
o=- 4611731400430051336 2 IN IP4 127.0.0.1\r
s=-\r
t=0 0\r
a=group:BUNDLE 0\r
a=extmap-allow-mixed\r
a=msid-semantic: WMS 6b8c3a6e-8d8f-4c39-9a8e-3b4c1f2d9e10\r
m=video 9 UDP/TLS/RTP/SAVPF 96 97 102 103 104 105 106 107 108 109\r
c=IN IP4 0.0.0.0\r
a=rtcp:9 IN IP4 0.0.0.0\r
a=ice-ufrag:Kq3P\r
a=ice-pwd:8nTf0vXkqzLs1yM2pR4a6bCd\r
a=ice-options:trickle\r
a=fingerprint:sha-256 7B:8B:F0:65:5F:78:E2:51:3B:AC:6F:F3:3F:46:1B:35:DC:B8:5F:64:1A:24:C2:43:F0:A1:58:D0:A1:2C:19:08\r
a=setup:actpass\r
a=mid:0\r
a=extmap:1 urn:ietf:params:rtp-hdrext:toffset\r
a=extmap:2 http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time\r
a=extmap:3 urn:3gpp:video-orientation\r
a=extmap:4 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01\r
a=extmap:9 urn:ietf:params:rtp-hdrext:sdes:mid\r
a=extmap:10 urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id\r
a=extmap:11 urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id\r
a=sendonly\r
a=msid:6b8c3a6e-8d8f-4c39-9a8e-3b4c1f2d9e10 0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0\r
a=rtcp-mux\r
a=rtcp-rsize\r
a=rtpmap:96 VP8/90000\r
a=rtcp-fb:96 goog-remb\r
a=rtcp-fb:96 transport-cc\r
a=rtcp-fb:96 ccm fir\r
a=rtcp-fb:96 nack\r
a=rtcp-fb:96 nack pli\r
a=rtpmap:97 rtx/90000\r
a=fmtp:97 apt=96\r
a=rtpmap:102 H264/90000\r
a=rtcp-fb:102 goog-remb\r
a=rtcp-fb:102 transport-cc\r
a=rtcp-fb:102 ccm fir\r
a=rtcp-fb:102 nack\r
a=rtcp-fb:102 nack pli\r
a=fmtp:102 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f\r
a=rtpmap:103 rtx/90000\r
a=fmtp:103 apt=102\r
a=rtpmap:104 H264/90000\r
a=rtcp-fb:104 goog-remb\r
a=rtcp-fb:104 transport-cc\r
a=rtcp-fb:104 ccm fir\r
a=rtcp-fb:104 nack\r
a=rtcp-fb:104 nack pli\r
a=fmtp:104 level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f\r
a=rtpmap:105 rtx/90000\r
a=fmtp:105 apt=104\r
a=rtpmap:106 H264/90000\r
a=rtcp-fb:106 goog-remb\r
a=rtcp-fb:106 transport-cc\r
a=rtcp-fb:106 ccm fir\r
a=rtcp-fb:106 nack\r
a=rtcp-fb:106 nack pli\r
a=fmtp:106 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f\r
a=rtpmap:107 rtx/90000\r
a=fmtp:107 apt=106\r
a=rtpmap:108 H264/90000\r
a=rtcp-fb:108 goog-remb\r
a=rtcp-fb:108 transport-cc\r
a=rtcp-fb:108 ccm fir\r
a=rtcp-fb:108 nack\r
a=rtcp-fb:108 nack pli\r
a=fmtp:108 level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f\r
a=rtpmap:109 rtx/90000\r
a=fmtp:109 apt=108\r
a=rid:l send\r
a=rid:m send\r
a=rid:h send\r
a=simulcast:send l;m;h\r
";

const FIREFOX_SPECTATOR_OFFER: &str = "v=0\r
o=mozilla...THIS_IS_SDPARTA-99.0 7419164441617433316 0 IN IP4 0.0.0.0\r
s=-\r
t=0 0\r
a=fingerprint:sha-256 2B:44:5E:8D:9C:14:71:62:7A:1D:C3:9E:6F:AA:01:2F:44:9B:7C:61:3D:D5:8E:20:6A:99:41:C0:B7:12:5E:3A\r
a=group:BUNDLE 0\r
a=ice-options:trickle\r
a=msid-semantic:WMS *\r
m=video 9 UDP/TLS/RTP/SAVPF 120 124 121 125 126 127 97 98\r
c=IN IP4 0.0.0.0\r
a=recvonly\r
a=extmap:3 urn:ietf:params:rtp-hdrext:sdes:mid\r
a=extmap:4 http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time\r
a=extmap:5 urn:ietf:params:rtp-hdrext:toffset\r
a=extmap:6/recvonly http://www.webrtc.org/experiments/rtp-hdrext/playout-delay\r
a=extmap:7 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01\r
a=fmtp:126 profile-level-id=42e01f;level-asymmetry-allowed=1;packetization-mode=1\r
a=fmtp:97 profile-level-id=42e01f;level-asymmetry-allowed=1\r
a=fmtp:120 max-fs=12288;max-fr=60\r
a=fmtp:124 apt=120\r
a=fmtp:121 max-fs=12288;max-fr=60\r
a=fmtp:125 apt=121\r
a=fmtp:127 apt=126\r
a=fmtp:98 apt=97\r
a=ice-pwd:4e4f7a0c3b2d1e6f8a9b0c1d2e3f4a5b\r
a=ice-ufrag:1a2b3c4d\r
a=mid:0\r
a=rtcp-fb:120 nack\r
a=rtcp-fb:120 nack pli\r
a=rtcp-fb:120 ccm fir\r
a=rtcp-fb:120 goog-remb\r
a=rtcp-fb:120 transport-cc\r
a=rtcp-fb:126 nack\r
a=rtcp-fb:126 nack pli\r
a=rtcp-fb:126 ccm fir\r
a=rtcp-fb:126 goog-remb\r
a=rtcp-fb:126 transport-cc\r
a=rtcp-mux\r
a=rtcp-rsize\r
a=rtpmap:120 VP8/90000\r
a=rtpmap:124 rtx/90000\r
a=rtpmap:121 VP9/90000\r
a=rtpmap:125 rtx/90000\r
a=rtpmap:126 H264/90000\r
a=rtpmap:127 rtx/90000\r
a=rtpmap:97 H264/90000\r
a=rtpmap:98 rtx/90000\r
a=setup:actpass\r
";

fn sfu() -> Sfu {
    let settings = Settings {
        port_min: 53_400,
        port_max: 53_499,
        public_ip: Some("203.0.113.7".parse().unwrap()),
        ipv6: false,
        relay: None,
    };
    Sfu::with_host_addresses(settings, vec!["127.0.0.1".parse().unwrap()])
}

fn video_section(sdp: &str) -> &str {
    &sdp[sdp.find("m=video").expect("a video section")..]
}

#[tokio::test]
async fn a_chrome_simulcast_camera_gets_an_answer_it_accepts() {
    let sfu = sfu();
    let (events, _receiver) = mpsc::unbounded_channel();
    sfu.join("t", "chrome", false, events).await.unwrap();
    let answer = sfu.offer("t", "chrome", CHROME_OFFER).await.unwrap();
    let video = video_section(&answer);

    assert!(video.contains("a=mid:0\r\n"));
    assert!(video.contains("a=recvonly\r\n"), "{answer}");
    assert!(answer.contains("a=group:BUNDLE 0\r\n"));
    assert!(
        answer.contains("a=setup:passive\r\n"),
        "the browser drives DTLS: {answer}"
    );
    assert!(
        answer.contains("a=rid:l recv") && answer.contains("a=simulcast:recv l;m;h"),
        "{answer}"
    );
    // H.264 Constrained Baseline in packetization mode 1, or VP8; nothing else, no audio.
    assert!(video.contains("a=rtpmap:106 H264/90000"), "{answer}");
    assert!(video.contains("a=rtpmap:96 VP8/90000"), "{answer}");
    assert!(
        !video.contains("a=rtpmap:104 "),
        "packetization mode 0 is refused: {answer}"
    );
    assert!(!answer.contains("m=audio"));
    // Host candidate on the room's socket, and the public address as server-reflexive.
    assert!(
        answer.contains(" 127.0.0.1 53") && answer.contains(" typ host"),
        "{answer}"
    );
    assert!(
        answer.contains(" 203.0.113.7 53") && answer.contains(" typ srflx"),
        "{answer}"
    );
}

#[tokio::test]
async fn a_firefox_spectator_is_answered_and_offered_the_chrome_board() {
    let sfu = sfu();
    let (chrome_events, _chrome) = mpsc::unbounded_channel();
    sfu.join("t", "chrome", false, chrome_events).await.unwrap();
    sfu.offer("t", "chrome", CHROME_OFFER).await.unwrap();

    let (events, mut receiver) = mpsc::unbounded_channel();
    sfu.join("t", "firefox", true, events).await.unwrap();
    let answer = sfu
        .offer("t", "firefox", FIREFOX_SPECTATOR_OFFER)
        .await
        .unwrap();
    assert!(
        video_section(&answer).contains("a=sendonly")
            || video_section(&answer).contains("a=inactive"),
        "{answer}"
    );
    assert!(!answer.contains("VP9"), "{answer}");

    let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let SfuEvent::Offer(offer) = event else {
        panic!("expected an offer, got {event:?}");
    };
    assert_eq!(
        offer["tracks"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap(),
        "chrome"
    );
    let sdp = offer["sdp"].as_str().unwrap();
    let board = &sdp[sdp.rfind("m=video").unwrap()..];
    assert!(board.contains("a=sendonly"), "{sdp}");
    assert!(
        board.contains("a=msid:chrome "),
        "the stream id names the board's owner: {sdp}"
    );
    assert!(
        board.contains("H264/90000") && board.contains("VP8/90000"),
        "{sdp}"
    );
    assert!(sdp.contains("a=group:BUNDLE 0 "), "{sdp}");
}
