# the-gathering-sfu

The webcam table's selective forwarding unit. Every seat (and spectator) holds one WebRTC
connection to it. Each seat publishes its camera as simulcast layers, and the SFU forwards
one layer of every other seat to each viewer. The table channel calls the `Sfu` methods in
`src/lib.rs`; the SFU answers with `SfuEvent`s whose payloads are pushed verbatim to the
browser.

## Design

- **[str0m](https://github.com/algesten/str0m) in RTP mode**, one `Rtc` per seat. str0m is
  sans-IO, so a room is a single tokio task (`room::run`) that owns its seats' connections,
  its UDP sockets, and the forwarding state. A publisher's packet reaches every viewer by a
  function call.
- **Crypto:** `str0m` with the `rust-crypto` feature (RustCrypto DTLS/SRTP via `dimpl`). No
  OpenSSL or libsrtp. Its certificate generation (`rcgen`) still pulls in `aws-lc-sys`, which
  needs only a C compiler at build time (`build-base` on Alpine; no CMake, Go, or Perl).
- **Sockets:** each room binds one UDP socket per interface address on the first free port in
  `port_min..=port_max` (not one per connection), announced as a host candidate, plus a
  server-reflexive candidate at `public_ip` when set. IPv4 only unless `ipv6`. Datagrams are
  demultiplexed to connections with `Rtc::accepts` (ICE ufrag, then source).
- **Relay-only mode** (`Settings.relay`): every connection allocates a relayed address on
  each TURN server the callback returns (`turn.rs`, `stun.rs`: Allocate with long-term
  credentials, CreatePermission for the browser's candidates, Refresh, Send/Data
  indications). Only `turn:` URLs over UDP are used; `turns:` and TCP are skipped.
- `subscription.rs`, `munger.rs`: per-viewer layer choice, duplicate filtering, and
  sequence-number/timestamp/VP8 picture-id rewriting, so a viewer switching layers sees one
  continuous stream.
- `simulcast_sdp.rs`, `browser_sdp.rs`, `ice_report.rs`: the SDP and logging helpers.

## Behavior

- The browser offers once, with its camera; every later offer comes from the SFU as boards
  are added or removed. str0m only reports remote media once SRTP is up, so the publisher is
  registered from that first offer.
- str0m never reports ICE `failed`; a connection that has not been connected for 10 s counts
  as failed and gets an ICE restart (new credentials, DTLS and tracks kept). After 3 restarts
  in 2 minutes the SFU sends `Down`, and the channel tells the browser to rejoin.
- Local candidates are in the answer SDP rather than trickled; relayed candidates allocated
  after the answer are trickled as `sfu_candidate`. Browser mDNS candidates are ignored
  (their checks still arrive as peer-reflexive pairs).
- Only H.264 Constrained Baseline (`42e01f`, packetization mode 1) and VP8 are offered, so
  every publisher's stream decodes at every viewer.
- A connection the browser closes (DTLS close) or that str0m fails on sends `Down`.
- The ICE report is built from what the room observes (addresses heard from, the address
  str0m sends to); connectivity-check counters are not available from str0m.

## Tests

`cargo test -p the-gathering-sfu`: unit tests, STUN/TURN
units, real-browser offer shapes (`tests/browser_offers.rs`), and end-to-end tests with str0m
clients over loopback (`tests/e2e.rs`; relay-only through a fake TURN server in
`src/relay_tests.rs`).
