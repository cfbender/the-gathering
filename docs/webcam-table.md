# Webcam table design

How the webcam table works inside: the realtime channel, the room that owns game state, the
media server, and card recognition. For what players see, read
[Features](features.md#webcam-table); for setting up networking, read
[Installation](installation.md#8-webcam-video-from-outside-your-network).

Short paths are relative to `rust/crates/the-gathering/src/` (server),
`rust/crates/sfu/src/` (SFU crate), or `assets/react/src/features/webcam-table/` (browser);
[Key files](#key-files) maps the main ones.

## Overview

A table is a temporary room for two to ten signed-in members, addressed by a UUID at
`/table/:roomId`. Each member sits as the player linked to their account, picks a deck, shares
a board camera, and records the result at the end. The recorded game is an ordinary game
(`POST /api/games`, the same endpoint as the manual form), so history and statistics need no
special cases.

```text
 browser ──── HTTPS ─────▶ /api/webcam-table/config   (ICE servers, SFU transport, socket token)
    │
    ├──── WebSocket ─────▶ /socket.io/ ─▶ table channel (one task per socket)
    │                                        │
    │                                        ├─▶ room task (one per table): seats, turns, timer,
    │                                        │      log, cards, votes; saved to SQLite
    │                                        └─▶ SFU room (one per table): signaling
    │
    └──── UDP (WebRTC) ──▶ SFU ports in WEBRTC_SFU_PORT_RANGE   (camera media)
```

The server holds the game state: browsers send intents, the room validates and saves them,
then broadcasts the result. Media never touches the socket or the HTTP proxy.

## The table channel

The realtime connection is Socket.IO (socketioxide on the server, `socket.io-client` in the
browser) on `/socket.io/`.

- **Authentication.** `GET /api/webcam-table/config` seals the cookie session's token into a
  one-day socket token that page scripts cannot read, and the socket presents it while
  connecting. Logging out disconnects the session's sockets.
- **Ordering.** Each socket's events go, in arrival order, to one task. A `join`
  (`{room_id, peer_id, player_id, deck_id}`) starts a table channel; later events go to it.
- **Replies.** Events that expect a reply are acknowledged with `{"ok": response}` or
  `{"error": reason}`.
- **Broadcasts.** Each table has a Socket.IO room. Every presence change sends it the full
  `presence` roster.

### Reconnects

The socket and the media connection fail independently, so a dropped socket does not cost the
seat. The server waits 30 s for a PONG, because the PING queues behind video on a saturated
downlink. A socket it gives up on keeps its seat, presence, and SFU connection for a 30-second
reconnect grace.

The browser rejoins with the same `peer_id`. If the server still has that peer's media
connection, it moves the seat and SFU connection to the new socket, re-sends any offer the old
socket missed, stops the old channel quietly, and replies `resumed: true`. The rest of the table
sees nothing. Otherwise it replies `resumed: false` and the browser builds a new media
connection under the same peer id.

When a channel fails because its room or SFU connection crashed, the browser gets `rejoin`
and joins again with a fresh peer id. `connection_unhealthy` (`reconnected` or `media`) warns a
seat whose connection is flaky.

### Who may do what

The authenticated player owns the seat, not the peer id. A newer connection for the same
player takes over the seat and stops the old channel. Another player cannot reuse a seat's
peer id.

- **Host controls** (`start_game`, `seat_order`, `arrange_seats`, `set_mode`, `turn_settings`,
  `adjust_turn`, `timer`, `end_game`, `rematch`) belong to the host and seated admins. Each
  seat's presence carries `host: true` when its connection holds them.
- While nobody holding the controls is present (a host inside the reconnect grace still
  counts), any seated player may send `timer`, `end_game`, `rematch`, and `start_game`, so a
  table whose host left can still finish and rematch. Mode, seat order, and turn corrections
  stay with the host.
- **Spectators** receive every board without a camera grant and cannot change the game.
- **Moderation.** Hosts and admins (even while spectating) can `remove_participant`, which
  blocks that player from the table across rematches and restarts, and `seat_spectator`, which
  sends the spectator `rejoin` so it is admitted like a new arrival.

### Limits

Socket messages are capped at 384 KiB, SDP at 64 KiB, a `peer_message` at 256 KiB, and a
virtual `board` at 128 KiB. An account may sit at three tables at once, because each table's
SFU holds ports from the shared range. Every event spends a token from a per-connection bucket
before it is validated; signaling has its own, larger bucket, because connecting sends dozens
of candidates at once. Joins and the config endpoint (which mints TURN credentials) are limited
per account (`rust/crates/the-gathering/src/config.rs`).

## Rooms and durable state

Each table is one tokio task (`rust/crates/the-gathering/src/webcam/room.rs`), started on the
first join. It serializes admission, so simultaneous last-seat joins cannot overfill the room,
and every mutation. Each mutation is saved to `webcam_table_sessions` before it is acknowledged
and broadcast, so nothing waits in a debounce window. The snapshot holds the whole game (seats,
counters, turns, timer, cards, log, vote, virtual boards), never media.

A room keeps running after its last connection leaves. Disconnecting never advances the turn or
drops a seat. A crashed room disconnects only its own channels, which rejoin and reload the
snapshot. This is a single-server design: there is no cross-node room coordination.

### Lifecycle

```text
 Games → Play / Join ─────────▶ /table/:roomId
                                     │
                    join: seat the linked player (≤ 10), or spectate
                                     │
                     choose deck · one WebRTC connection each to the SFU
                                     │
                       start_game: the roster locks
                                     │
                                 End game
                                     │
             ╭───────────────────────┴───────────────────────╮
   record result (POST /api/games)                  skip recording
             ╰───────────────────────┬───────────────────────╯
                                     │
             ╭───────────────────────┴───────────────────────╮
      end_game: room closes                    rematch: same room resets to a lobby
  every seat gets table_closed                     nobody navigates
```

- **Start.** `start_game {randomize}` keeps the shown order or shuffles it (whole teams in
  Two-Headed Giant). It locks the roster: returning players reclaim their seats, newcomers
  spectate.
- **End.** `end_game` deletes the snapshot and stops the room; every channel pushes
  `table_closed`, so browsers leave instead of rejoining a fresh lobby under the same id. A tab
  that missed it rejoins with `rejoin: true`, which the server refuses for a table with no
  snapshot ("This table has ended.").
- **Rematch.** `rematch` resets the same room in place, so browsers keep their
  cross-origin-isolated document (see [below](#cross-origin-isolation)). Owner, mode, and the
  connected players' seats and decks persist; the game state resets, and each seat gets
  `seat_reset` to rehydrate its local controls.
- **Pruning.** Every minute the pruner closes rooms with no connections and no activity for
  30 minutes. Snapshots expire seven days after their last write; running rooms refresh theirs
  hourly, so expiry only clears tables a restart left behind.
- **Listing.** `GET /api/webcam-table/rooms` (`web/channels/rooms.rs`) lists every running
  room. Seated channels track themselves on a lobby presence topic that supplies each room's
  players. The Games page polls it every 15 s to show Play, Join, or a menu of tables.

### Game state

- **Seats.** Players publish their own changes with `update_status` (life −999…999, camera,
  poison, rad, commander casts and damage, custom counters, combat effects). The channel
  validates each field (counters 0…999, maps of at most 100 entries, names of 1…300 bytes) and
  merges it into presence. Life at zero or below eliminates the seat; gaining life back does
  not restore it.
- **Modes** (`Mode` in `webcam/mod.rs`): Commander (40 life), Two-Headed Giant (even roster of
  at least four, adjacent seats are teams with 60 shared life), Five Star (exactly five; the
  "can't attack yet" badges are advisory), Pauper Commander (30 life), and the one-on-one modes
  at 20 life: Duel Commander, Standard, Pauper, Canadian Highlander. Only the Commander formats
  are recorded; the others end or rematch without a game record. Two-Headed Giant and Five Star
  lock seat order at start, because team life and turns are keyed by position.
- **Turns** (`webcam/turns.rs`) advance in seat order and skip eliminated seats.
  `pass_turn` and `unpass_turn` carry the last seen revision, so simultaneous passes advance
  once. The last 20 passes can be undone. Per-player time is banked against game time, so
  pausing freezes every clock.
- **Timer** (`webcam/timer.rs`). Only the server writes `started_at`, `paused_at`, and
  `paused_ms`. Browsers interpolate from a server sample with `performance.now()` and resync
  every 15 s with `timer_sync`. A running timer keeps counting through server downtime.
- **Log** (`webcam/log.rs`). The room writes entries, keeps the newest 200, and merges an
  actor's repeated change of one kind within five seconds into one line.
- **Rolls and votes.** The server generates `roll` results and stamps the roller, so clients
  cannot forge them. Secret votes (`webcam/vote.rs`) never send individual choices to a browser.
- **Identified cards** (`webcam/cards.rs`). Seats send `cards` commands (`card_identified`,
  `card_removed`, `cards_cleared`); the room keeps up to 500 entries and broadcasts
  `identified_cards` with the acting seat, stamping the player name itself. Identifications
  made during a private reveal stay local.
- **Virtual board.** A seat can play on the
  [`@cfbender/playtest`](https://github.com/cfbender/playtest) tabletop instead of a camera. The
  game stays in the owner's browser; only its public projection (hand and library as counts)
  goes to the room as `board`.

### Recording a game

The End game form pauses the timer and builds a normal game payload
(`game-result.ts`): played-at from the shared start, a duration rounded
to whole minutes, and every seat in the shared order, including eliminated ones. It posts to
`POST /api/games`, whose handler drops provenance fields and records through
`games/record_game.rs`. The game schema accepts 2–10 players, matching the table's ten seats.

## Media: an SFU with simulcast

Every browser holds one `RTCPeerConnection` to the server's SFU
(`rust/crates/sfu`, built on [str0m](https://github.com/algesten/str0m)), not one per other
seat. A peer-to-peer mesh made each browser run a 1080p encoder per opponent: on a real
four-seat game in Chrome that cost 150–200% of a core per browser and a TURN allocation per
pair. With the SFU, each browser encodes once and decodes N−1 mostly small streams.
`rust/crates/sfu/README.md` covers the crate's internals.

**Signaling** rides the table channel. The browser's first `sfu_offer` carries its camera;
every later offer comes from the server as boards come and go, with a `tracks` map of mid →
owner peer id, and is answered with `sfu_answer`. `sfu_candidate` trickles ICE both ways.
Spectators add a receive-only transceiver so the server has a connection to offer boards on.

**One room per table.** The SFU room is one tokio task owning one str0m `Rtc` per seat and
forwarding RTP between them. str0m never reports ICE failure itself; a connection that has not
been connected for 10 s gets an ICE restart (new credentials and candidates, DTLS and tracks
kept), which the browser sees as a blip. After three restarts in two minutes the room gives
up, and the channel tells the browser to rejoin.

**Simulcast.** Each seat publishes three layers (`simulcastEncodings` in
`use-sfu-connection.ts`). Layer `h` is what `videoEncoding` in `media-policy.ts` allows for the
room size and quality setting (1080p at 2.5 Mbps for up to four seats); `m` halves the
resolution and quarters the bitrate; `l` halves and quarters again. Frame rate is 30 for two
seats and 15 otherwise.

**The viewer picks the layer.** `watchTile` observes every remote `<video>` with a
`ResizeObserver` and `layerForHeight` asks for `l` up to 270 device pixels, `m` up to 540, and
`h` above (`sfu_layer {peer_id, layer}`, sent only on change). A rail thumbnail costs a small
decode while the pinned board stays sharp.

**Switching layers** (`subscription.rs`, `munger.rs`). A switch waits for a keyframe of the new
layer, which the room requests with a PLI rate-limited to one per 300 ms per layer. The munger
rewrites sequence numbers and timestamps so the viewer's decoder sees one continuous stream. A
viewer whose layer pauses moves to the nearest live one. It moves back up after the wanted
layer is steady for 2 s, but down to a softer wanted layer only after 5 minutes, because Chrome
drops and restores its lowest layer every few minutes under CPU pressure. A 256-packet window
drops packets the browser resends over RTX, since forwarding one twice fails the viewer's SRTP
replay check.

**Codecs.** The SFU offers H.264 Constrained Baseline and VP8; the browser ranks H.264 first
(`orderVideoCodecs`), because it has hardware support on most platforms and measured about half
VP8's CPU even in software. The SFU forwards whatever each publisher sends. Re-offers restore
`a=rid` / `a=simulcast` from the browser's first offer (`simulcast_sdp.rs`) so it keeps sending
three layers.

**Private reveal is enforced on the server.** The `reveal` event limits a publisher's video to
one viewer (`Sfu::reveal`) and the SFU stops forwarding it to everyone else, so hidden video
never leaves the server. The target leaving ends the reveal. Hidden viewers cannot request a
crop either.

## ICE, STUN, TURN, and proxies

Settings are in [Installation](installation.md#8-webcam-video-from-outside-your-network) and
its [configuration reference](installation.md#configuration-reference). The reasoning:

- **The HTTPS proxy carries only the socket.** It must pass WebSocket upgrades for
  `/socket.io/`. Media is UDP between each browser and the SFU, which no HTTP proxy can front.
- **Sockets.** Each SFU room binds one UDP socket per interface address on the first free port
  in `WEBRTC_SFU_PORT_RANGE` and announces those as host candidates, plus `WEBRTC_SFU_PUBLIC_IP`
  as a server-reflexive candidate when set. With a port forward and a public address, the
  server needs no STUN, and because the server end has a fixed public port, even symmetric
  home NATs connect directly. Without the public address, only browsers on the same LAN can
  reach the server.
- **IPv4 only by default.** The server offers IPv4 host candidates unless `WEBRTC_SFU_IPV6` is
  on (`host_addresses` in `rust/crates/sfu/src/net.rs`). The port forward and
  `WEBRTC_SFU_PUBLIC_IP` are IPv4, browsers hide their IPv6 host addresses behind mDNS names,
  and a LAN browser that picked the server's IPv6 candidate has been seen to lose its media.
- **Browser STUN** defaults to Google's and Cloudflare's public servers so a browser behind NAT
  learns its reflexive address. The SFU ignores browsers' mDNS candidates; their checks still
  arrive as peer-reflexive pairs.
- **TURN for browsers** that cannot send UDP to the SFU: a static relay or Cloudflare Realtime
  TURN. `rust/crates/the-gathering/src/cloudflare_turn.rs` exchanges the long-lived key for
  per-join credentials that outlive a game, and passes on only the UDP 3478 and TLS 443 URLs,
  because a browser opens one allocation per URL and Firefox warns at five or more servers. The
  config endpoint appends Cloudflare's servers after the static ones, and a Cloudflare outage
  degrades to the static list instead of blocking the room.
- **Relay-only mode** (`WEBRTC_SFU_RELAY_ONLY`) is for hosts that cannot forward ports: the
  SFU dials out to Cloudflare TURN for every seat (`turn.rs`, `stun.rs` in the SFU crate) and
  all media crosses the relay. The config endpoint tells the browser which transport is in use
  (`sfu.transport`).
- **Diagnosis.** On a failed connection the server logs an ICE report (`ice_report.rs`: every
  candidate pair and when it was last heard) and the browser logs its pairs to the console.

## Card recognition

Clicking a card on any board identifies it in the clicking browser. Confirming a candidate adds
it to that board's card tray for every seat.

### The bundle

The recognizer is a bundle of three ONNX graphs (detector, embedder, search) plus gallery
metadata (`arts.json`, optionally `printings.json`), built and published from
[Oracle](https://github.com/cfbender/oracle). Nothing model-related is committed here or baked
into the image. Oracle's `cardid.publish --to github:cfbender/the-gathering` creates a
`scanner-bundle-<version>` release of this repository, never marked latest, so it does not
affect self-update.

`rust/crates/the-gathering/src/card_id/update.rs` checks at boot and every six hours. When the
newest release's version differs from `current`, it downloads every file into
`DATA_DIR/cardid/.incoming`, verifies sizes and SHA-256 sums against the manifest, moves it into
place, and swaps `current`; the old one becomes `previous`. Rolling back means unpublishing the
newest release. `GET /api/cardid/bundle` returns the manifest and file URLs, and
`GET /api/cardid/bundles/:version/:name` serves immutable files, so browsers cache them by
version. A `404` from the manifest means no bundle: clicks on your own board then suggest your
commanders instead.

Gallery building and training belong to Oracle; see its README and
[CONTRIBUTING.md](https://github.com/cfbender/oracle/blob/main/CONTRIBUTING.md).

### Inference in the browser

`recognition/recognizer.worker.ts` runs the graphs with ONNX Runtime Web on its WASM backend.
`recognition/pipeline.ts` is a port of Oracle's Python reference runtime (`cardid/bundle.py`)
and gives the same top five. The first click or gallery search starts the worker
(`recognition/use-recognizer.ts`); entering a room loads nothing. Each identify has a
2-second timeout, which starts only after loading.

Running in the clicker's browser costs nothing extra: the clicker already holds the crop, each
browser loads the bundle once, and the result is ready to show, correct, and announce without a
second round trip. Server-side inference would make the application host the hot compute path
and upload camera images.

**WebGPU was evaluated and rejected for now.** The native WebGPU execution provider has no
`Round`, `Mod`, or `Or` kernels, so the embedder bounces full images between GPU and CPU; even
with those ops rewritten, a click took about 1,900 ms on WebGPU against about 150 ms on WASM in
Firefox. WebGPU is also missing from many browsers, so WASM must stay as the fallback anyway.

### Cross-origin isolation

onnxruntime-web runs WASM on several threads only when the worker is `crossOriginIsolated`,
which cut a click from about 250 ms to about 100 ms on a desktop and roughly tripled phone
speed. Isolation is scoped to the table, and three things must hold:

- **The table document** (`/table` and `/table/*` in `web/mod.rs`) is served with
  `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`
  (`require-corp` because Safari lacks `credentialless`). The rest of the app is not isolated,
  because COEP would block Discord avatars and other third-party images. The table loads
  nothing cross-origin (card images come through `/api/card-images`); keep it that way, or give
  a new cross-origin resource CORS or a `Cross-Origin-Resource-Policy` header.
- **Worker scripts** need `Cross-Origin-Embedder-Policy: require-corp` too, or the browser
  refuses to start them and reports only an empty `ErrorEvent`. `web/statics.rs` adds it to
  `/assets/react` in production and `vite.config.ts` in development. The worker loads
  onnxruntime's `ort-wasm-simd-threaded.mjs` with a `?coep=1` cache key, because browsers kept a
  year-long immutable copy from before the header; bump it if those headers change.
- **Crossing `/table/` loads a new document**, because the headers belong to the document.
  Links use `reloadDocument`, and the root route turns any other crossing into a full load
  (`assets/react/src/lib/cross-origin-isolation.ts`).

A threaded start can fail or hang (a pthread worker whose script was blocked never reports in).
The worker fetches the WASM binary itself and starts the runtime under a 20 s `initTimeout`.
On failure `use-recognizer.ts` starts a fresh worker on one thread, since onnxruntime cannot
initialize twice in one worker.

### Native-resolution crops

Recognition needs the camera's full resolution, but a viewer may be decoding a small simulcast
layer. Each seat publishes the height its camera actually delivers (`camera_height`) in
presence, and `use-card-capture.ts` decides:

1. If the clicker's decoded frame of that board is at least `camera_height` rows tall (for
   example the pinned board's top layer in a room of up to four), it crops locally with no
   round trip.
2. Otherwise it sends the click as normalized coordinates in a `peer_message` to the camera
   owner. The owner maps them to its native frame, crops 640 px around the click, re-encodes
   the JPEG coarser until it fits the 256 KiB relay cap, and returns it with the click position
   inside the crop. A seat only answers for its own camera, and only for viewers allowed to see
   it.

Video flips are viewer-only, so the click is mapped back to unflipped coordinates before the
request, and the returned crop is mirrored to match the viewer's flip (`orient-crop.ts`). A crop
that reads back pure black means the browser's graphics state broke: it is not recognized, and
the owner answers `capture_blank`, so the clicker's warning names whose browser to restart.

### From candidates to the tray

- A top match that leads the runner-up by `CLEAR_MARGIN` (0.08 cosine,
  `card-suggestions.tsx`) is recorded at once and opens the card preview. Anything closer opens
  the picker: five numbered candidates, the crop with the detected outline, and a gallery
  search.
- A best match on a blank gallery art (`BLANK_ARTS` in `recognition/pipeline.ts`) means the
  card was seen but not identified, usually because of glare. It records nothing and opens the
  picker empty.
- **Outlining** (Shift+click four corners, `outline-drawing.ts`) skips the detector for cards it
  misses. The outline also teaches the detector: its correction is marked
  `quad_source: "manual"`.
- **Deck-list prior** (`deck-hint.ts`). When the board owner's deck links to Moxfield, Archidekt,
  or ManaVault, every seat loads the list from `GET /api/decks/:id/decklist` and warms its
  images. Each top-five candidate in the list gets `DECK_PRIOR` (0.03) before re-ranking, and
  the matching printing is recorded when the gallery holds it. A deck card outside the top five
  is not recovered; that would need a new search-graph input. Opponents' lists are not shown in
  the UI but are visible in the network tab.

### Corrections

An explicit picker choice (never an automatic answer) posts the native crop, click, outline,
chosen gallery id, and the original ranking to `POST /api/cardid/corrections`
(`use-correction-upload.tsx`). Both the clicker and the camera owner must allow sharing, and
crops from a private reveal are never sent. The server stores them under
`DATA_DIR/cardid/corrections` and never prunes them; new captures get `507` once a member
reaches `CARDID_CORRECTIONS_MAX_PER_MEMBER` (default 2000) or the directory reaches
`CARDID_CORRECTIONS_MAX_MB` (default 1024). Exporting and training from them are described in
Oracle's README, "Training from in-app corrections".

## Layout

The page is a play surface rather than a video-call grid:

```text
┌────────┬──────────────────────────────────┬──┬────────────┐
│ camera │                                  │  │ Table      │
│ rail:  │   active board: follows the      │▪ │ Decks      │
│ every  │   turn, or the board you pinned  │▪ │ Cards      │
│ seat,  │   (click = identify a card)      │▪ │ Log        │
│ small  ├──────────────────────────────────┤  │ Settings   │
│        │ name · life · commander · menu   │  │            │
└────────┴──────────────────────────────────┴──┴────────────┘
```

Grid view replaces the rail and active board with every camera at once. Below the `lg`
breakpoint, the rail becomes a strip of seat chips and the side panel a bottom sheet. Layout
and view preferences are per player and per browser, not room state. `/table/*` routes render
without the app header and force the dark theme (`TableShell` in
`assets/react/src/routes/__root.tsx`) so popovers match the black stage.

## Key files

Server (`rust/crates/the-gathering/src/` unless noted):

| Path | Role |
| --- | --- |
| `web/channels/` | Socket.IO setup and tokens (`mod.rs`), one seat's channel (`webcam_table.rs`), presence, open-tables list |
| `webcam/` | Room registry and pruning (`mod.rs`), the room task (`room.rs`), snapshots (`session.rs`), turns, timer, log, cards, votes |
| `web/api/webcam.rs`, `cloudflare_turn.rs` | Config endpoint: ICE servers, transport, socket token |
| `card_id/`, `web/api/cardid.rs` | Bundle updater, bundle serving, corrections |
| `rust/crates/sfu/` | The media server (see its README) |

Tests: `rust/crates/the-gathering/tests/integration/webcam_*.rs`, and the SFU crate's unit tests
and `tests/e2e.rs`.

Browser (`assets/react/src/features/webcam-table/`):

| Path | Role |
| --- | --- |
| `table-channel.ts`, `use-room-channel.ts` | Socket connection, join and rejoin |
| `use-webcam-room.ts` | Composes the room: channel, camera, SFU connection, game state |
| `use-sfu-connection.ts`, `media-policy.ts` | The peer connection, simulcast, layer requests, reveal |
| `use-table-game-state.ts` | Seat status, timer, turns, and order from the server |
| `use-card-capture.ts`, `recognition/`, `card-suggestions.tsx` | Crops, the recognizer worker, the picker |
| `finish-game.tsx`, `game-result.ts` | End game form and recorded payload |
| `webcam-table-page.tsx`, `table-hotkeys.tsx` | Layout; keyboard shortcuts (`?` lists them) |
