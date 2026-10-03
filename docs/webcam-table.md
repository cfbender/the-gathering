# Webcam table

## Product slice

A webcam table is a temporary room for two to ten signed-in members. Each member is seated as
the player linked to their account, chooses a deck while in the room, shares their board camera,
and ends the game with a result form that records through `Games.create_game/2`. The resulting
`Game` and `GamePlayer` rows are deliberately
indistinguishable from a manually logged game, so existing history and statistics need no
special cases.

The owner selects a game mode before starting. Commander is the default. Two-Headed Giant
Commander requires an even roster of at least four: adjacent seats form teams, randomization
shuffles whole pairs, each team starts at 60 shared life, and turns/counts/timing are shared.
Use the lobby's seat arrows to arrange teammates. Shared life reaching zero eliminates the whole
team; eliminating or restoring a player also applies to their whole team. Counters and
commanders remain individual. The result picker records both teammates as winners.

Five Star requires exactly five seats. Each seated viewer sees “Can't attack yet” on their
original previous/next neighbours. An eliminated neighbour loses its badge, and all badges clear
once only three players remain. These are
advisory badges, not enforced attacks or alliances. Spectators see no restrictions. Seat order
is fixed after starting either new mode, including across reconnects.

Recorded games store `format` (`commander`, `two_headed_giant`, or `five_star`), also selectable
in the manual game form. Existing games and imports default to Commander. Run `mix ecto.migrate`
when upgrading to apply the `add_format_to_games` migration. Room snapshot version 2 stores mode
and team life; version 1 snapshots continue loading as Commander.

Card clicks assist the room rather than define its durable record. A click on any board fetches
a native-resolution crop from the camera owner's browser, runs the card recognizer in the
clicking browser, and shows five numbered candidates plus a gallery search; confirming one posts
an "identified" line to every seat's log. The recognizer bundle is published to the server from
[Oracle](https://github.com/cfbender/oracle) (see **Recognition** and Oracle's README); when the server has none, the panel falls back
to your own known commanders as deck-based suggestions (only on clicks over your own board, since
only a seat's owner may choose its commander).

## Decisions

### A server-side SFU with simulcast

Every browser holds one `RTCPeerConnection` to the server (`TheGathering.WebcamTables.Sfu`,
built on `ex_webrtc`), not one to each other seat. Phoenix Channels carry authenticated presence
and the signaling for that single connection: the browser's first `sfu_offer` carries its camera
and every later offer comes from the server (`sfu_offer` with a `tracks` map of mid → owner peer
id) and is answered with `sfu_answer`; `sfu_candidate` trickles ICE in both directions.
`Sfu.Room` is one GenServer per table owning one `ExWebRTC.PeerConnection` per seat; it adds a
send-only track per other publisher (stream id = owner's peer id, so the browser can map a
remote stream to its seat), and forwards RTP between them. If a seat's connection fails the room
first re-offers with an ICE restart (new credentials and candidates, DTLS and tracks kept, so the
browser sees a blip rather than a rejoin); after three restarts in two minutes it stops the
channel and the browser rejoins with a fresh peer id.

The table used to be a full mesh. Measured on a real four-seat game in Chrome 154, each browser
ran three 1080p software encoders plus three decoders at 150–200% of a core, rebuilt the encoders
on every adaptation step, and burned one TURN allocation per pair; the SFU exists to cut that to
one encode and N−1 mostly tiny decodes per browser.

Each seat publishes its camera **once, as three simulcast layers** (`simulcastEncodings` in
`use-sfu-connection.ts`): rid `h` is what `videoEncoding` in `media-policy.ts` allows for the room
size and quality setting (1080p at 2.5 Mbps for up to four seats), `m` halves the resolution and
quarters the bitrate, `l` halves and quarters again (480 × 270 at ~156 kbps from a 1080p camera).
Frame rate is 30 for a two-seat table and 15 otherwise. The viewer, not the publisher, decides
which layer it receives: `watchTile` observes every `<video>` drawing a remote stream with a
`ResizeObserver`, takes the tallest 16:9 fit in device pixels, and `layerForHeight` asks for `l`
up to 270 px, `m` up to 540 px, and `h` above that (`sfu_layer {peer_id, layer}`, sent only when
the answer changes). A rail tile therefore costs a quarter-resolution decode while the pinned
board stays sharp, and pinning another board switches within a keyframe.

`Sfu.Subscription` is the pure per-viewer-per-publisher state: which layer is live, which is
pending, and an `ExWebRTC.RTP.Munger` rewriting sequence numbers and timestamps so the viewer's
decoder sees one continuous stream across switches. A switch waits for a keyframe of the new
layer; the room asks the publisher for one with a PLI on that rid, rate-limited to one per
300 ms per layer so several viewers switching at once do not make the browser spend its whole
bitrate on keyframes. A viewer showing nothing yet adopts a keyframe of any layer rather than
staying black while its wanted layer is paused under CPU pressure. Browsers also resend recent
packets over RTX to probe bandwidth and `ex_webrtc` recovers those into the original packets
again, so the subscription keeps a 256-packet window of forwarded sequence numbers and drops the
duplicates (forwarding one twice fails the viewer's SRTP replay check).

Two `ex_webrtc` 0.17 gaps are worked around here. Its re-offers never carry `a=rid` /
`a=simulcast` for a receiving transceiver, so after the second seat joined the browser silently
dropped to a single layer; `Sfu.SimulcastSdp` captures those attributes from the browser's first
offer and restores them in the copy of each server offer sent to the browser (the server keeps
its own unaltered SDP because `set_local_description` rejects a changed one). And the server
answers a browser offer that includes H.264 and VP8 with both; the browser ranks H.264 first via
`setCodecPreferences` (`orderVideoCodecs`) for the reasons below, and the SFU forwards whatever
codec each publisher ends up with, since every WebRTC browser decodes both.

One cost is not worked around: `ex_webrtc` generates a fresh RSA-2048 DTLS certificate for every
peer connection inside a regular (non-dirty) `ex_dtls` NIF, which blocks one BEAM scheduler for
50–500 ms per join (`:erlang.system_monitor` `long_schedule` on `ExWebRTC.DTLSTransport.init/1`).
On a four-core host a full table arriving at once stalls other work by that much; it is why
`test/test_helper.exs` raises ExUnit's `assert_receive_timeout`. Fixing it needs an upstream
option to supply a pre-generated certificate or a dirty-scheduler NIF.

H.264 is the codec hardware encoders and decoders cover (VideoToolbox on macOS and iOS, Media
Foundation on Windows, VA-API on Linux once Chrome's accelerated-video flags are on), and even in
software it is cheaper: in a three-seat 1080p/15 fps room in Chrome 154, each browser dropped from
~105% of a core with libvpx to ~55% with OpenH264 encode and FFmpeg decode. `chrome://webrtc-internals`
(`about:webrtc` in Firefox) shows the result as the codec and `encoderImplementation` /
`decoderImplementation` of each stream.

Reveals are enforced server-side: `Sfu.reveal/3` limits a publisher to one viewer and the room
stops forwarding to everyone else, so hidden video never leaves the server. The native crop RPC
that used to ride each pair's data channel is a targeted channel event instead
(`peer_message {to, message}` in, `peer_message {from, message}` out, capped at 256 KB, so the
owner re-encodes a crop coarser until it fits); a seat still only answers crop requests for its
own camera, and a viewer whose `<video>` already decodes the board at the owner's advertised
`camera_height` skips the RPC and crops its own frame. Spectators add a receive-only
transceiver so the server has a connection to offer boards on without a camera.

The room keeps one UDP socket per connected browser from `WEBRTC_SFU_PORT_RANGE` and announces
`WEBRTC_SFU_PUBLIC_IP` as its server-reflexive address when set, so a host that forwards that
range needs no STUN of its own. `WEBRTC_SFU_RELAY_ONLY=true` instead makes the server connect
out through Cloudflare TURN (`ice_transport_policy: :relay`) for hosts that cannot forward ports,
at the cost of every byte crossing the relay. `GET /api/webcam-table/config` tells the browser
which (`sfu.transport`).

Phoenix's own Channels documentation confirms that signaling is application-defined and that
signed-token authentication belongs in `connect`; the authenticated config endpoint signs the
existing tracked session token for the socket handshake. MDN documents SDP and ICE exchange
through such a signaling service. References:

- <https://hexdocs.pm/phoenix/channels.html>
- <https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Signaling_and_video_calling>
- <https://github.com/elixir-webrtc/ex_webrtc>

### ICE, STUN, TURN, and proxies

The normal HTTPS reverse proxy must pass WebSocket upgrades for `/socket`; it never carries RTP.
Media is UDP between each browser and the SFU's ports, which no HTTP proxy can front: forward
`WEBRTC_SFU_PORT_RANGE` from the router straight to the host running the app and set
`WEBRTC_SFU_PUBLIC_IP` to the router's WAN address so the server's candidates are reachable.
Without the public address the server offers only its interface addresses, which works on one
LAN and leaves remote browsers at "Connecting…" forever. On the browser side `WEBRTC_STUN_URLS`
defaults to public STUN servers (Google and Cloudflare) so a browser behind NAT learns its own
reflexive address; override it or set it to `none` for a LAN-only install. Because the server
end has a fixed public port, ordinary and even symmetric home NATs connect directly; browsers on
networks that block UDP entirely need a TURN server configured with `WEBRTC_TURN_URLS`,
`WEBRTC_TURN_USERNAME`, and `WEBRTC_TURN_CREDENTIAL`, which cannot be hidden behind an HTTP
reverse proxy either (expose 3478 and preferably TURN-over-TLS on 5349/443 from coturn or
another relay). A failed connection is labelled "Couldn't connect" on every remote tile; the
server logs an ICE report (role, candidates, and every pair with how long ago it was last heard)
and restarts ICE as described above, and the browser logs its own candidate pairs to the console.
Credentials are returned only from the authenticated config endpoint.

The server offers IPv4 candidates only unless `WEBRTC_SFU_IPV6=true`. Browsers hide their host
addresses behind mDNS names, and `ex_ice` resolves those only to IPv4, so a browser that picked
the server's IPv6 candidate streamed from an address the server had never learned and `ex_ice`
silently dropped every packet: the browser believed it was connected while the server timed the
pair out eight seconds later. The port forward and `WEBRTC_SFU_PUBLIC_IP` are IPv4 anyway.

Hosts that cannot forward ports set `WEBRTC_SFU_RELAY_ONLY=true` with a Cloudflare TURN key:
the server then dials out to the relay for every seat and all media crosses it, roughly 20 GB per
relayed 1080p player in a three-hour game.

The hosted alternative is Cloudflare Realtime TURN: set `CLOUDFLARE_TURN_KEY_ID` and
`CLOUDFLARE_TURN_API_TOKEN` (create the key under Realtime → TURN in the Cloudflare dashboard) and
`TheGathering.CloudflareTurn` exchanges that long-lived key for per-join credentials via
`POST https://rtc.live.cloudflare.com/v1/turn/keys/:id/credentials/generate-ice-servers`. The
credentials expire after `CLOUDFLARE_TURN_TTL_SECONDS` (default six hours, longer than a game;
refreshing mid-session would need `RTCPeerConnection.setConfiguration`). Cloudflare answers with
six TURN URLs (primary and alternate ports for UDP, TCP, and TLS); only `turn:…:3478?transport=udp`
and `turns:…:443?transport=tcp` are passed on, because a browser opens one relay allocation per
URL on every peer connection and Firefox logs "Using five or more STUN/TURN servers slows down
discovery" once the whole list reaches five. With the two default STUN servers that makes four
URLs. The config endpoint appends Cloudflare's servers after the static ones, dropping URLs the
static list already covers, and falls back to the static list with a logged warning if
Cloudflare is unreachable, so a
Cloudflare outage degrades to STUN-only rather than blocking the room. Only pairs that cannot
connect directly use the relay; Cloudflare bills relayed egress at $0.05/GB after the first
1,000 GB each month (STUN at `stun.cloudflare.com` is free and unlimited). One relayed 1080p
player in a four-seat, three-hour game is roughly 20 GB.

### Browser inference in the clicking browser

The detector, perspective warp, art crop, embedder, and cosine search run in the browser of the
player who clicked, on the crop the camera owner already returns over the channel relay (see the
next section). Running it there rather than in the owner's browser costs nothing extra: the crop
transfer already solves the resolution problem, each browser loads the bundle once, and the
result needs no second round trip before it can be shown, corrected, and announced. ONNX Runtime
Web (`onnxruntime-web`) executes the three exported graphs on its WASM backend; the glue around
them (`recognition/pipeline.ts`) is a line-for-line port of the Python `cardid.bundle` reference
runtime, and the two give the same top five on rendered scenes.

#### WASM threads and cross-origin isolation

onnxruntime-web 1.30 runs WASM on several threads (pthreads over `SharedArrayBuffer`) only when
the worker is `crossOriginIsolated`. A 640 px card frame (two detector passes, embed, search)
went from 252 ms on one thread to 99 ms on four in headless Chromium on an 8-core machine, and
ManaVault's identical change took a phone from ~190 ms to 50–70 ms.
The worker sets `ort.env.wasm.numThreads` to 0 (onnxruntime picks `min(4, ceil(cores / 2))`)
when isolated and 1 otherwise, reports the count it initialized with in its `ready` message,
and the Connection panel shows it (`… loaded in 0.8 s, 4 threads.`). onnxruntime cannot
initialize twice in one worker, so if a threaded start fails (`load_failed` or a worker
`error`), `use-recognizer.ts` terminates that worker and starts a fresh one on one thread.
A threaded start can also hang rather than fail: onnxruntime waits for a `loaded` message from
every pthread worker, and a worker whose `.mjs` the browser refused (an extension or policy
blocking the URL) never sends one; Chrome reports that as a worker `error`, Firefox reports
nothing. The worker therefore fetches `ort-wasm-simd-threaded.wasm` itself alongside the model
files and passes it as `ort.env.wasm.wasmBinary`, then starts the runtime under
`ort.env.wasm.initTimeout` (20 s, compile and thread start-up only, not the 14 MB download).
A timeout is a `load_failed`, so the same one-thread retry runs and the panel ends on
"failed" instead of "Loading…" forever.

Isolation is scoped to the table and needs three things to hold:

- **The table document** (`GET /table/*`, before the SPA catch-all in `router.ex`) is served
  with `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy:
  require-corp` by `TheGatheringWeb.CrossOriginIsolation`. `require-corp`, not
  `credentialless`, because Safari/iOS lacks the latter. Nothing else is isolated: the rest of
  the app shows Discord avatars and other third-party images that COEP would block. The table
  loads nothing cross-origin (card images come through `/api/card-images`, captures are data
  URLs, and WebRTC media is not a subresource); keep it that way, or give new cross-origin
  resources CORS (`crossorigin`) or a `Cross-Origin-Resource-Policy` header.
- **Worker scripts** must carry `Cross-Origin-Embedder-Policy: require-corp` too, or the
  browser refuses to start a dedicated worker inside the isolated document and reports only an
  `ErrorEvent` with an empty message ("worker crashed"). Vite's `server.headers` adds it in
  development (to files Vite serves, not responses proxied from Phoenix) and the
  `/assets/react` `Plug.Static` adds it in production. That covers the recognizer worker and the
  pthread workers onnxruntime spawns from the standalone `ort-wasm-simd-threaded.mjs`.
  Those assets are `immutable` for a year under content-hashed names, and the `.mjs` hash
  predates the header, so browsers that visited earlier kept a copy without COEP and Firefox
  refused to start pthread workers from it ("blocked by policy"). The worker therefore appends
  a cache key (`?coep=1`) to the `.mjs` URL; bump it if these asset headers change again.
- **Navigation across `/table/`** must load a new document, because the headers belong to the
  document: a client-side hop in would not be isolated, and one out would leave the games list
  isolated. Links across it use `reloadDocument`, and the root route's `beforeLoad` turns any
  other crossing (for example the End game redirect or a sign-in `returnTo`) into a full load
  (`lib/cross-origin-isolation.ts`).

WebGPU was evaluated with onnxruntime-web 1.30 and rejected for now; the recognizer stays on
WASM. Findings, from the three graphs exported with the production operator set:

- The default `onnxruntime-web` import uses the older JSEP WebGPU backend, which fails in the
  detector (`[Concat] /Concat failed: non concat dimensions must match`).
  `onnxruntime-web/webgpu` (asyncify) and `onnxruntime-web/jspi` load the native WebGPU EP, which
  runs all three graphs and matches WASM output.
- The native WebGPU EP has no `Round`, `Mod` or `Or` kernel. Those nodes fall back to the CPU
  EP; in `embed.onnx` that means 15 full-image GPU→CPU→GPU copies. Rewriting them at export
  (round-half-to-even from `Floor`/`Where`, a table lookup for the corner roll, float
  arithmetic for `peak % size`) leaves only uint8 input slices and a few int64 index ops on the
  CPU and does not change WASM output.
- Measured on Firefox 156 / Linux (WebGPU behind `dom.webgpu.enabled`): WASM ~146 ms per click
  (2 × detector + embed + search), WebGPU ~1,900 ms even with the rewritten export. Times sat
  near multiples of 100–200 ms regardless of graph size, which points to per-readback latency
  rather than compute.
- WebGPU is on by default only in Chromium, Safari 26, and Firefox on Windows and Apple Silicon
  macOS, so WASM must remain the fallback anyway. Revisit only with per-device backend
  selection (for example, timing the load-time warm-up on both) and measurements from those
  browsers.

References:

- <https://onnxruntime.ai/docs/tutorials/web/>
- <https://onnxruntime.ai/docs/tutorials/web/ep-webgpu.html>

Do not run inference in Phoenix: Ortex/Nx would make the application host the hot compute path,
complicate CPU portability, and upload imagery. A Python sidecar could remain in one container,
but would add a second supervised runtime and duplicate the spike runtime in production.

### Remote clicks use the source camera's native frame

`getUserMedia` requests a hard minimum of 1920 × 1080, and each seat publishes the rows its
camera actually delivers as `camera_height` in presence (with `shares_corrections`, its
training-upload consent). A click on a remote tile is cropped locally when the clicker's
`<video>` for that board has a decoded frame at least `camera_height` rows tall: in rooms of up
to four the pinned board's top simulcast layer is the unscaled camera, so the crop is identical
to the owner's and costs no round trip. Otherwise (a rail tile or grid cell decoding a lower
layer, the top layer still ramping up, or a publisher capped to 720p/540p) the click is sent as
normalized coordinates in a targeted `peer_message` channel event. The camera owner's browser
maps the coordinates to its native `videoWidth`/`videoHeight`, captures the same 640 px JPEG crop
used by `cardid.capture` (re-encoded coarser until it fits the 256 KB relay cap), and returns it
the same way together with the click position inside the crop. The requester recognizes the card
from that crop. Either way the crop carries the owner's privacy (`reveal_to`) and consent, not
the clicker's.

Video flips are a viewer-only preference (players can flip their own preview too; the sent
stream is never flipped), so the click is mapped back to unflipped source
coordinates before it is sent, and the owner always crops native pixels. The requester then
mirrors the returned crop and its click position to match its own flip of that board
(`orient-crop.ts`), so recognition, the picker thumbnail, and correction uploads see the card as
it appears on screen.

This protocol does not depend on the resolution selected by WebRTC congestion control (a
960 × 540 received stream still yields a crop of the owner's 1920 × 1080 frame, by asking) and keeps the
click-to-candidate latency budget local: capture + detector + embedder + gallery search, with no
server image round trip.

## Lifecycle and ownership

```text
Games page Play / Join button ────▶ /table/:roomId
                                         │
                             auto-seat linked player
                                         │
                              choose deck in the room
                                         │
                       one WebRTC connection each to the SFU (≤ 10)
                                         │
                                 click End game
                                         │
                ╭────────────────────────┴───────────────────────╮
       complete result form                  End (or Rematch) without recording
                │                                                │
POST /api/games → Games.create_game/2                            │
                │                                                │
                ╰───────────── After this game? ─────────────────╯
                                         │
                ╭────────────────────────┴───────────────────────╮
     Close the table: end_game                     End and rematch: rematch
       room closes for everyone               same room resets to a fresh lobby
                │                                                │
 recorded: saved game page · otherwise: games list      nobody navigates
```

Rooms are UUID-addressed and durable in SQLite's `webcam_table_sessions`. The serialized state
server loads a room lazily on join and commits a versioned server-owned snapshot before acknowledging
each mutation. Seats, life/counters/damage, selected commanders, elimination, monarch, order,
turn counts/times, timer, identified card lists and the table log survive reloads and server
restarts. Media is not stored. Immediate writes avoid a debounce data-loss window;
this remains a single-server design, not a distributed room coordinator.

Ending the game closes the room. After the owner records the result, or chooses End without
recording (confirmed inline), the browser sends `end_game`. `WebcamTables.close/1` deletes the
snapshot and stops the room with `{:shutdown, :closed}`. Every joined channel sees that exit on its
room monitor, pushes `table_closed`, and stops normally, so browsers leave instead of rejoining a
fresh room under the same id. Other seats return to the games list with a notice. The table drops
off the Play/Join list at once, and its seats no longer count as taken.

**End and rematch** is the form's other *After this game* choice. It records the result (**Record
and rematch**) or skips it (**Rematch without recording**, confirmed inline), then sends `rematch`
instead of `end_game`. `WebcamTables.rematch/1` resets the same room to a fresh lobby and nobody
navigates, so the table keeps its cross-origin-isolated document. What persists: the room id,
owner, game mode, auto-randomize setting, and the seats of players still connected (or within the
ten-second reload grace), in their last seat order with their decks, commanders, camera and reveal
state. What resets: the timer (back to setup, not started), turns and turn times, each seat's life
(40), poison, rad, commander tax and damage, eliminations, Two-Headed Giant team life, the monarch,
identified cards, and the log (which restarts with one "Rematch" line). Seats of players who have
left are dropped, so the lobby holds exactly who is present; spectators keep watching and take a
seat by reloading while the new lobby is open. The room broadcasts `table_state` and `table_log`
to every seat and sends each seated connection `{:seat_reset, seat}`; the channel adopts that seat
in its assigns and presence and pushes `seat_reset` so the browser rehydrates its local life and
counters. If recording succeeds but the rematch does not, the form still closes (the game is saved)
and the owner can retry with Rematch without recording.

Rooms keep running after their last seat leaves. `TheGathering.WebcamTables.Pruner` runs every
minute: it closes rooms that have had no connections and no activity (joins, saved changes or
disconnects) for 30 minutes, deleting their snapshots, and deletes snapshots that have expired.
Snapshots expire seven days after their last write; running rooms refresh theirs hourly, so expiry
only matters for sessions left behind by a server restart. Joins reject expired snapshots.
Finished games use the same idle policy; recording a result does not delete the room. A closed or
expired UUID opens a fresh lobby.

The authenticated player ID owns the seat, not the transient peer ID or Presence entry. A newer
connection takes over the same seat, stops the old channel, and remaps order, monarch and cards.
Each channel retry uses a fresh media peer ID so the browser rebuilds its SFU connection rather than
keeping a mismatched one after a signaling restart. Video elements are muted (there is no
table audio), allowing spectators to autoplay without a camera grant or a prior click.
Disconnecting does not advance the turn. The first timer start (through `start_game` or legacy
`seat_order`) locks the roster: returning players reclaim their seats, everyone else spectates.
Spectators receive boards/cameras without requesting camera permission and cannot mutate the game.
They stay in Presence (with `spectator: true`) but take no seat; the Table tab lists them under
turn order, and the Setup header shows their count.
The first seated player owns table setup, timer and turn-count corrections; players retain their
own life/counter/commander controls. Any seated player can pass the turn.

Join replies carry the authoritative seat, and `table_state` includes `seats`, `owner_id`,
`monarch` and `cards`. Clients refresh the one-day socket token (encrypted with
`Phoenix.Token.encrypt`, so page scripts cannot read the session token inside it) on connection
failure. Game state survives restarts: rooms reload their saved session on the next join.

The server bounds untrusted input: `peer_id` must be a canonical UUID (clients use
`crypto.randomUUID()`), SDP in `sfu_offer`/`sfu_answer` is capped at 64 KB and a relayed
`peer_message` at 256 KiB, and the websocket refuses frames over 384 KiB. Every channel event spends a token from a per-connection bucket
(signals have their own, larger bucket) and replies `{reason: "rate limited"}` when it is empty;
joins and TURN credential requests (`GET /api/webcam-table/config`) are limited per account.
Limits live under `config :the_gathering, TheGatheringWeb.RateLimit`.

The Games page still finds open tables without the URL: `GET /api/webcam-table/rooms`
(`TheGatheringWeb.WebcamTableRooms`) lists every running room, and every seated channel process
also tracks itself on one lobby presence topic that supplies each room's connected players (join
order, `full` at ten seats). `PlayActions` (`features/webcam-table/play-actions.tsx`) polls it
every 15 s: with no open table the header shows **Play**; with one it shows **Join** naming the
seated players (or "Empty table") plus a smaller **New table**; with several, Join becomes a menu
of tables. An empty room stays listed until the pruner closes it; after a server restart, rooms
reappear once someone opens their saved URL.

## Table view layout

The room is laid out like a webcam play surface rather than a video-call grid: by default one
board is large, everyone else is small, and controls live in a collapsible column. Grid view
(Settings → View, or `G`) shows every camera at once instead.

```text
┌──────────┬─────────────────────────────────────────────┬──┬──────────────┐
│ rail     │ 40                                          │  │ Setup  3/10  │
│ ┌──────┐ │                                             │▪ │ Invite       │
│ │40    │ │                                             │▪ │ Commander    │
│ └──────┘ │              active board                   │▪ │ Turn order   │
│ Mara ⋯ 📷│           (click = identify card)           │  │ Randomize    │
│ Select…  │                                             │  │ End game     │
│ ┌──────┐ │                                             │  │ Leave table  │
│ │37    │ │                                             │  ├──────────────┤
│ └──────┘ │                                             │  │ Identify  ▸  │
│ Cody ⋯ 📷│                                             │  │ Connection ▸ │
│ Open seat├─────────────────────────────────────────────┤  │              │
│          │ Theo ⋯                       📷  Select cmd │  │              │
└──────────┴─────────────────────────────────────────────┴──┴──────────────┘
```

- **Camera rail** (left, `lg:` defaults to 15 rem): every seat as a 16:9 tile with one life control
  over the video and a single-line name / ⋯ menu / camera indicator / commander bar. Hover or
  focus your life box (tap on touch screens) to reveal stacked ±1 buttons and the counters chevron.
  Your life box is also a text field: type a new total and press Enter (or click away) to apply it
  as one change; Escape or a non-number discards the edit.
  Other seats have read-only life and an always-visible chevron to inspect their counters.
  Once a commander is chosen, a tax badge (`+4`, or `+4/+2` for partners in name order) sits just
  left of the commander name on every seat; adjust it from the counters panel or tax hotkeys.
  The ⋯ menu offers flip video (vertical or horizontal, your own preview included) and
  eliminate/restore for every seat; your own menu also has
  camera on/off and Reveal hand, opening the existing private-reveal flow in a dialog.
  Commander names truncate when necessary; the full name remains in the title/hover preview.
  Empty seats up to ten render as dashed "Open seat" placeholders. The rail scrolls vertically
  on desktop and horizontally in 240px tiles at narrow widths rather than shrinking ten cameras until their
  names and controls are unreadable. Clicking a tile soft-pins that board over the active turn;
  clicking the same tile again releases it.
- **Commander identity** colors both the rail and active-board name bars: one muted solid for
  mono-color, a WUBRG-ordered gradient for multiple colors, and neutral for colorless or unknown.
  The active-board commander name shows identity pips using the existing mana symbols; compact
  rail bars omit pips and the YOU tag to preserve name space at minimum width. The source is the decks
  API's stored `color_identity` (including partners), not a browser Scryfall request. An empty
  identity is treated as unknown, not falsely labelled colorless; explicit `C` shows its pip.
- **Active board** (center): the stage fills the remaining viewport. The same life control sits
  top-left and the matching name bar underneath carries the seat menu, camera state, and "Select
  commander" popover without repeating life. The stage follows the active turn; before turns
  start it shows the newest remote joiner, and when the shown player leaves it falls back to your
  own board. While a board is soft-pinned, a Follow turn button top-right releases it. Clicking the
  video starts the click-to-identify flow, and the suggestion card floats bottom-center over the
  stage (keys 1–5 still pick).
- **Grid view** replaces the rail and active board with every seat's camera in a near-square
  grid (Two-Headed Giant teams stay together under their shared life). Tiles letterbox rather
  than crop so each whole board stays visible. Clicking a tile fills the stage with that board,
  restoring the rail and card identification, until the same tile or the Back to grid button is
  clicked. A soft pin belongs to the view it was made in, so switching views starts unpinned.
- **Side panel** (right): a narrow icon strip (Table, Decks, Cards, Log, Settings) plus a collapse chevron and shortcut help. The
  Table tab holds the Setup section (elapsed-time badge and players count in the header, Invite
  players copies the room URL, Select your commander, a turn-order table with #/Player/Turn/Time
  (life and commander under the name, and up/down arrows for the owner), then before the match
  the primary Start match button beside Randomize, and after it Pass turn, Un-pass and Pause/Resume
  timer; Reveal hand to…, the red End game button, Leave table), dice/coin controls, and collapsed Identify
  cards and Connection sections. Decks lists your commanders; Log shows the table event log. Collapsing the
  panel leaves only the icon strip so the board grows.
- **Resize dividers** on desktop drag the camera rail (176–360 px, default 240) and panel content
  (240–480 px, default 288); widths also cap at 24vw / 32vw to preserve board space. Double-click
  resets one divider. Focus a divider and use Left/Right to resize by 16 px, Home to reset.
  Settings offers a reset for both widths and a hotkeys toggle. Preferences persist per player
  in this browser under `the-gathering:table-preferences:<playerId>`; they are not room state.
- **Keyboard shortcuts** (Convoke-compatible where the table has the same feature):
  `Space` passes the turn after the match starts and `Shift+Space` un-passes it; `↑` / `↓` gains/loses one life and
  `Shift+↑` / `Shift+↓` gains/loses ten life (always your own seat). `[` / `]` subtracts/adds
  two commander tax by changing your primary commander's cast count by one; partner commanders
  retain individual counter rows. `C` toggles your camera, `B` collapses/expands the panel,
  `T` / `D` / `A` / `L` / `S` opens Table / Decks / Cards / Log / Settings, and `,` / `.`
  soft-pins the previous/next board, and `G` toggles grid view. `?` or `H` toggles the grouped
  shortcut dialog.
  All table actions, including Space, honor the enable preference and pause while typing,
  using a keyboard widget, or while an overlay/picker is open. Ctrl/Alt/Meta, composing and
  repeat events are ignored; Space preserves native button/link activation. The card picker
  retains `1`–`5` and `/` gallery search. Escape closes overlays even with shortcuts disabled.
- **Settings** stacks collapsible Keyboard shortcuts, View, Camera, Sound and Card scan sections.
  View switches between following the active turn and the all-cameras grid; clicking or cycling
  a board soft-pins it without changing the saved view. Left/Right swaps the side panel and camera rail on desktop;
  narrow layouts keep cameras above and controls below. Glass/Classic uses the existing global
  theme-style preference. Camera lists available devices, remembers the choice and enabled state,
  and replaces outgoing tracks on existing peer connections without leaving the room. Camera-off state and
  private-reveal restrictions survive switching. Missing saved cameras fall back to the system
  default with a warning. Capture requests ideal 1080p (lower-resolution devices are accepted).
  Publisher quality offers Auto (the existing seat-count tiers), 1080p, 720p or 540p ceilings;
  it changes sender scaling/bitrate without lowering native card-crop resolution, and the
  seat-count frame-rate cap applies to every choice. Stats sample
  each connection every two seconds while enabled: remote tiles show received resolution, fps,
  bitrate and the selected remote ICE candidate type; the local tile shows native capture
  resolution/fps (no network hop). Check video health reports local track settings and state.
  Turn sound is an opt-in WebAudio tone, unlocked by interaction, on transitions to your turn.
  Card scan contains recognition bundle status/version and the existing corrections-sharing
  opt-out (`the-gathering:share-card-corrections`). Other table preferences use the per-player
  browser key above; sound starts on, stats start off, and the view starts on follow-turn. There is no microphone, hand-count
  or token-copy binding because those features do not exist here.
- The `/table/*` routes force the dark theme (`TableShell` in `routes/__root.tsx` swaps
  `data-theme` on mount and restores the user's choice on unmount) so portalled popovers and
  dialogs match the black stage. They render no application header.

### Creating and editing commanders in the room

The commander popover and Decks tab offer **New commander…** and, for the selected deck,
**Edit commander…** to its owner or an administrator. The dialog reuses the Decks feature's card
search and printing picker. The optional second card can be a partner, Background, companion, or
casual pairing; the table does not enforce deck-building legality. The deck name follows the
commander names until edited. Saving creates or updates the player's ordinary deck through the
decks API, then selects it in the room without navigation. Editing changes the saved deck, not
just this session's appearance.

Both names appear in the rail, board bar, and turn order. `DeckSummary.commander_image_url` and
`partner_image_url` expose full-card images, preferring the selected printing and falling back to
the catalog; the existing `*_art_crop_url` fields use the same printing preference for crops.
After a validated `choose_deck`, the channel broadcasts `deck_selected` so every browser refreshes
its deck query, including when the same deck ID is selected after an art or partner edit.

### Shared seat state

Presence metadata carries, per seat, `life` (starts at 40), `camera_off`, `eliminated` (false), and a
server-stamped `joined_at`. Players publish their own changes through the channel's
`update_status` event (validated: life −999…999, camera boolean, and counters below; no other keys) and the channel merges
them into presence, so every browser shows the same totals without another round trip. Your own
life is also tracked locally so rapid ± clicks compound before presence echoes back. On every
rejoin, authoritative life and counters hydrate those local controls before editing resumes.

The chevron below each life box opens **Counters**. Everyone can inspect a seat; only its
owner can change its counters. `poison` and `rad` start at zero. `commander_casts` maps commander
names to command-zone cast counts. Explicit −2 / tax / +2 rows replace the small art tax buttons,
with art thumbnails and separate rows for partners/backgrounds. These still publish ±1 cast deltas and display twice
the cast count, bounded at zero and 999 casts. Poison, rad, commander damage and the monarch action
share this panel. Other players' tax is read-only. Custom counters are not supported.
Commander-name text uses the deck's color identity (gold for multicolor). Both commander
and partner/background names come from the selected deck. `commander_damage` maps opposing
player IDs to commander-name/count maps, keeping identical commanders at different seats separate.
Recorded damage stays visible when a source changes deck or leaves. Damage does not adjust life
automatically. Poison at 10 and damage of 21 from any one commander are flagged in red; damage
from separate commanders is never combined for the threshold. The server accepts only integers
0…999, maps of at most 100 entries, and names of 1…300 bytes. Counter changes use optimistic local
deltas and restore from the server on rejoin and full page reload.

**Take the monarch** claims the crown for your own seat. `take_monarch` has an empty payload;
The room process serializes claims and broadcasts one `monarch` holder, never per-seat flags.
Late joiners receive `monarch_state`; server revisions prevent stale snapshots from replacing
newer claims. A crown appears on the holder's tile and active board and survives disconnects
and server restarts along with the rest of the room snapshot.
Counter changes and monarch transfers are added to the shared Log.

Once the match has started, the room owner can eliminate or restore any present seat, and each
player their own, from the ⋯ seat menu on the video tile or board bar; nothing is eliminable in
the lobby.
`set_eliminated` validates a present peer ID and a boolean; the target channel merges it into
its own presence so later life/camera updates cannot overwrite elimination. Players may also
publish their own `eliminated` through `update_status`. In every format, a life update that
reaches zero or below marks the seat eliminated automatically; gaining life back does not restore
it, so a player is brought back only through an explicit restore. Out seats stay visible with dimmed video,
an Eliminated badge, and a struck-through name. They receive no order number; only eligible seats
are numbered, while the recording order still includes everyone. Eliminating
the current player advances the turn to the next eligible seat; disconnecting does not. If none remain there is no active
turn; restoring a player starts their next turn. Counts and accumulated time are kept.

Eliminated seats are retained in the room state and shared through `eliminated_seats`, so
leaving does not drop them from the result or from a late joiner's view. Rejoining as the same
player takes back the retained position and elimination flag. A departed seat must rejoin before
it can be restored. All seats survive the last disconnect in the durable snapshot. The End game form
suggests the only non-eliminated player as winner (when at least two players took part), with all
others defaulting to losses in recorded seat order. The result remains editable; choosing Draw
explicitly records the whole table as draws, as required by the existing game schema.

Default turn order is join order (`joined_at`, then peer id) so every browser agrees. Before the
match the owner can move any seat with the up/down arrows in the turn-order table (`arrange_seats`),
then either **Start match** (`start_game {randomize: false}`, keeps the shown order) or
**Randomize** (`start_game {randomize: true}`, shuffles the present peers — whole pairs in
Two-Headed Giant — then starts). A bare `start_game {}` still falls back to the shared
`turn_settings` option `auto_randomize`, which older clients may set. After the start the arrows
stay available in Commander only and push `seat_order`; Two-Headed Giant and Five Star lock their
order at start because team life and turns are keyed by seat position. The channel verifies each
list names exactly the retained seated peers (never spectators), then broadcasts it. Rows shuffle
visibly for about one second before settling into a randomized order (instant with reduced motion,
no animation for an ordered start or a manual move). The End game form numbers seats in that order
and records them the same way. Reordering mid-game **never resets or resumes the timer or changes
the current turn**.

Any seat can use **Pass turn** or **Space**. Turns advance in the shared order, skip eliminated seats and
wrap around. Temporarily disconnected seats keep their turns. TURN counts increment when a turn starts, including the first
turn; small −/+ controls send `adjust_turn` corrections (0–999) without changing time or active
player. `pass_turn` requires the last seen turn revision, so simultaneous passes only advance
once. TIME is the player's accumulated time in m:ss, including their current turn. The server
banks time against game elapsed time rather than wall time, so pausing freezes both game and
player clocks; passing while paused changes the turn but adds no paused time. A single remaining
player can continue taking turns; elimination does not automatically end the game.

Any seat can also **Un-pass** (**Shift+Space**) to undo a mistaken pass. The turn state keeps the
last 20 passes (who passed, when their turn started, who received it); `unpass_turn` takes the
same revision as `pass_turn` and pops the newest one: the previous player's turn resumes from its
original start, the time banked by the pass is removed, and the receiver's TURN count drops by one.
Time spent since the pass counts toward the resumed turn. Repeated un-passes walk further back.
The server refuses when the newest pass did not lead to the current turn (for example after every
seat went out and play restarted) or when the previous player has since been eliminated or left.

An amber dot and highlight mark the current player's row; an amber Current turn badge marks their
rail tile and board. This is independent of the violet active-board border. Space does
not pass while typing, using a control, holding a modifier other than Shift (which un-passes), repeating a key, composing text, or
while a card picker/dialog is open. It shares the guarded registry and enable preference in
`table-hotkeys.tsx` with the other table shortcuts.

Each room process serializes its shared timer, turns and order.
The first valid `seat_order` starts it. Only the room owner can send `timer` with `pause` or `resume`;
only the server writes `started_at`, `paused_at`, and accumulated `paused_ms`. The timer bar
above the active board's name bar derives elapsed time excluding pauses. Browsers interpolate
from a server sample using `performance.now()`, not their wall clock, and resync every 15 seconds
with `timer_sync` (half-round-trip latency compensation). New/rejoining seats receive the
current timer, order, settings, counts and per-player times via `table_state`. A running timer
includes server downtime; a paused timer remains paused. Multi-node room state is not supported.

End game pauses the timer for everyone and captures that server response for the result form.
Duration is editable, prefilled in whole minutes rounded to nearest (minimum one minute, matching
the game schema). Without a started timer it stays blank. Going back leaves the timer paused;
the owner can resume explicitly. `played_at` uses the shared start timestamp when available.

The Table tab offers d6, d20, custom dice with 2–1000 integer sides, and coin flips. The `roll`
event validates the request, generates the result on the server, stamps it with the authenticated
seat's name/id and server time, then broadcasts to everyone. Results appear in a five-second
overlay and in the Log. Clients cannot supply a result or impersonate the roller.

The Log tab is owned by the room (`TheGathering.WebcamTables.Log`): the room process writes
entries for joins and leaves, deck/life/camera/counter changes, eliminations, the monarch, seat
order and rolls, keeps the newest 200 in the saved snapshot, and broadcasts each new or merged
entry as `log_entry`. Every (re)join receives the whole log as `table_log`, so all seats see the
same history and a reload restores it. A reload or reconnect within ten seconds logs neither a
leave nor a join. Consecutive events of the same kind and actor within two seconds coalesce
(merged entries keep their id): life keeps the first and final totals, dice/coins retain every
result, and deck/camera changes show the latest state with a count. Different actors, event
kinds, and intervening entries break the group.

Audio is not part of the webcam table: no microphone is captured and there are no mute
controls. Players use their usual voice app alongside the table.

### Private hand reveal

Choose another seated player in the Table tab's **Reveal hand to…** action, and wait for the confirmation before
showing your hand. Per-peer cloned video tracks are immediately disabled for non-targets, then
their senders use `replaceTrack(null)`; the native camera and target's sender stay live.
Late joiners also start with no outgoing video track. Presence carries `reveal_to`, validated
by the channel as another current peer, so hidden seats render **Revealing to name** instead of
a video element and the target sees **name is revealing to you**. The owner still sees their
own camera. Camera off overrides reveal and stays off when reveal ends.

**End reveal** restores eligible senders in one click. A target departure automatically ends
the reveal in both channel presence and the owner's media policy, restoring public video;
put the hand down before ending or leaving. Signaling reconnects preserve the local restriction.
Admission is serialized so simultaneous final-seat joins cannot overfill the room. Another player
cannot reuse a seat's peer ID; the same authenticated player replaces their old connection.

Hidden viewers cannot identify the board: both the requesting UI and the owner's native-crop
handler enforce visibility. Authorized viewers still receive the owner's native 640 px JPEG
crop from the untouched 1080p source, unaffected by the sender tiers. Identifications during
a reveal stay local to the clicker rather than entering the shared card tray. A chosen viewer
can still save or share what they saw; this feature cannot revoke frames already delivered.

## File and component structure

- `TheGatheringWeb.UserSocket` issues and decrypts a short-lived encrypted token wrapping the tracked
  cookie session.
- `TheGatheringWeb.WebcamTableChannel` caps rooms at ten, carries SFU signaling and targeted
  `peer_message`s,
  merges `update_status`/`set_eliminated` into presence, validates `seat_order`/`timer`/`timer_sync`,
  `start_game`/`turn_settings`/`pass_turn`/`unpass_turn`/`adjust_turn`, and generates
  and broadcasts validated `roll` results.
- `TheGathering.WebcamTables` is the context API for admission and game state. Each room is a
  `WebcamTables.Room` process (one per room, started on first join under
  `WebcamTables.RoomSupervisor`, registered in `WebcamTables.Registry`) that loads its
  `WebcamTables.Session` snapshot on start, saves every change before broadcasting it, and stops
  when its last connection leaves. A crashing room only disconnects its own channels, which rejoin
  from the saved snapshot. `WebcamTables.Pruner` deletes expired sessions hourly; running rooms
  refresh their own expiry (covered by `webcam_table_channel_test.exs`).
- `TheGathering.WebcamTables.Turns` owns pure turn advancement, elimination skipping, counts and
  accumulated-time accounting, and `WebcamTables.Timer` the pause-aware game clock
  (`test/the_gathering/webcam_tables/`).
- `TheGatheringWeb.Presence` owns ephemeral room membership and seat status.
- `WebcamTableConfigController` exposes authenticated ICE configuration and the SFU transport mode.
- `TheGathering.WebcamTables.Sfu` (`Sfu.Room`, `Sfu.Subscription`, `Sfu.SimulcastSdp`) is the
  media server: one room process per table, one `ExWebRTC.PeerConnection` per seat, per-viewer
  layer selection and packet rewriting, and the simulcast SDP repair
  (`test/the_gathering/webcam_tables/sfu/`).
- `features/webcam-table/use-sfu-connection.ts` owns the browser's one peer connection: simulcast
  publishing, remote streams keyed by owner, `watchTile`/`layerForHeight` layer requests, and the
  `peer_message` relay; `stream-tiles.ts` hands `watchTile` to `StreamVideo` through context.
- `features/webcam-table/use-webcam-room.ts` composes camera, SFU connection, native crop RPC,
  seat status (life, camera, elimination), seat order, timer/turn synchronization, roll overlays, and the event log.
- `features/webcam-table/use-correction-upload.tsx` uploads explicit picker labels and owns
  the crop-sharing preference/save note. Both camera owner and clicker must allow sharing.
- `features/webcam-table/webcam-table-page.tsx` composes the rail, stage, and side panel and owns
  the active-board selection (`useActiveBoard`).
- `features/webcam-table/board.tsx` — `ActiveBoard`, `CameraTile`, `OpenSeat`,
  elimination/current-turn overlays, and `capturePoint` (click → normalized coordinates).
- `features/webcam-table/life-control.tsx` — hover/focus life controls and counter entry point.
- `features/webcam-table/seat-bar.tsx` — the name/actions/camera/commander bar under a board or tile.
- `features/webcam-table/commander-picker.tsx` — popover listing your own decks; other seats see a
  read-only commander label, and the server rejects `choose_deck` for decks you do not own.
- `features/webcam-table/card-suggestions.tsx` — the click-to-identify overlay: crop with the
  detected quad, five numbered candidates, gallery search, timings, and "In deck" marks.
- `features/webcam-table/seat-decklists.ts` — loads every seated deck's linked list and warms its
  images through a small concurrency-limited queue; `deck-hint.ts` is the pure deck-list prior
  (`applyDeckHint`, `deckFirst`, exact-printing lookup); `decklist-dialog.tsx` is **Decks →
  View decklist**. `features/decks/decklist-cards.ts` holds the `GET /api/decks/:id/decklist`
  query and deck-builder grouping.
- `features/webcam-table/recognition/` — `use-recognizer.ts` (hook owning the worker and its
  checking/loading/ready/unavailable/failed state), `recognizer.worker.ts` (ONNX Runtime Web
  sessions, warm-up, identify and search), `pipeline.ts` (pure port of Oracle's `cardid/bundle.py`:
  window resample, detector refine pass, upright vote, gallery search parsing; unit-tested in
  `pipeline.test.ts`), and `messages.ts` (worker protocol types).
- `TheGathering.CardId` + `CardIdBundleController` serve the published bundle from
  `DATA_DIR/cardid/current` (`GET /api/cardid/bundle` for the manifest and file URLs,
  `GET /api/cardid/bundles/:version/:name` for the immutable files). `404` means no bundle is
  published and the UI falls back to deck suggestions.
- `features/webcam-table/side-panel.tsx` — icon strip and Table/Decks/Cards/Log tabs
  (`cards-tab.tsx` holds the Cards tab; `panel-section.tsx` the collapsible section).
- `features/webcam-table/finish-game.tsx` — the End game dialog: record the result or skip it, then
  either close the table or reset the same room for a rematch.
- `features/webcam-table/game-result.ts` — winner suggestion and normal recorded-game payload,
  including eliminated seats (`game-result.test.ts`).
- `features/webcam-table/table-timer.tsx` — elapsed-time badge in the Table tab header plus the
  pause/resume action; both appear once the match has started.
- `features/webcam-table/use-timer-elapsed.ts` — shared monotonic timer display hook.
- `features/webcam-table/seat-order-table.tsx` — animated order, current-turn highlight, counts,
  per-player time and elimination controls.
- `features/webcam-table/turns.ts` — next-seat suggestion and turn display (`turns.test.ts`).
- `features/webcam-table/table-hotkeys.tsx` — guarded table bindings and grouped help dialog.
- `features/webcam-table/table-settings.tsx` — collapsible browser preferences and camera controls.
- `features/webcam-table/camera.ts`, `video-stats.tsx`, `use-turn-sound.ts` — device acquisition,
  connection sampling and opt-in turn notifications.
- `features/webcam-table/game-timer.ts` — elapsed-time interpolation, formatting, and duration
  prefill helpers (`game-timer.test.ts` also covers roll descriptions).
- `features/webcam-table/table-rolls.tsx` — dice/coin controls, wire types, and result descriptions.
- `features/webcam-table/table-events.ts` — pure helpers for log lines, seat ordering, and
  shuffling, active-turn filtering, departed eliminated seats, and event coalescing
  (unit-tested in `table-events.test.ts`).
- `features/webcam-table/board-cards.tsx` — the card tray docked to the bottom of the active board
  and the shared `CardThumb`.
- `features/webcam-table/card-preview.tsx` — the card image + rules text overlay; `card-details.ts`
  fetches one printing's details from `GET /api/card-printings/:id/details`.
- `routes/table.new.tsx` and `routes/table.$roomId.tsx` are thin route adapters.

The finish mutation posts the normal game payload (`played_at`, optional duration/turns/win
condition/notes, and consecutive seats with player/deck/result) to `/api/games`. The controller
already strips provenance fields and delegates to `Games.RecordGame`.
`Game` accepts 2–10 distinct participants with consecutive seats; `GamePlayer` accepts seat
numbers 1–10 (and up to nine kills). The scrollable End game dialog and turn-order table both
render every participant in the shared order.

## Recognition

The recognizer ships as a **bundle** exported and published from [Oracle](https://github.com/cfbender/oracle) (`cardid.export`,
`cardid.publish`; see its README, "Shipping"). Phoenix serves whatever
`DATA_DIR/cardid/current` points at; nothing model-related is committed to this repository or
baked into the container image, so a new bundle (new model, or the same model with a refreshed
gallery after a set release: `mise run new-set` in Oracle) is a `publish` away and browsers pick it up on their next table
because they cache bundle files by version.

To train a better model, try one at a local table, or contribute crops and Shift+click outlines,
see Oracle's [CONTRIBUTING.md](https://github.com/cfbender/oracle/blob/main/CONTRIBUTING.md).
A bundle published with `--to ../the-gathering/data/cardid` from a sibling Oracle checkout is
served by a local dev server as-is.

Gallery coverage includes paper artwork in any language (including Japanese-only alternate
art), prepare cards, meld cards, and both scanned sides of transform/MDFC/reversible cards
and double-faced tokens. Separate sides use face names and IDs `<scryfall UUID>` (front)
and `<scryfall UUID>-1` (back); prepare/adventure share one art and keep their combined name.
Rooms, classic split, aftermath and flip cards also expose two named halves with those IDs.
Their gallery crops come from regions of the whole front scan, rotated upright before
embedding. Both visible halves can appear as candidates; this does not infer unlocked Room
doors or a flip card's active rules. Details show the chosen half's rules with the shared
whole-card image, not a fictitious reverse image. Cycling alternate printings keeps the same
split/flip half selected; full-card deck-picker requests retain combined names. Art series, battles and novelty splits
with more than two parts remain excluded. See Oracle's README, **Gallery coverage,
printings and face IDs**, for crop geometry and the refresh/export/publish commands. Deploying this
code alone does not rebuild the gallery bundle.

The gallery uses Scryfall's **all_cards** metadata to attach all paper printing/language
siblings to each distinct illustration, instead of letting `unique_artwork` choose the
only selectable printing. Regular-frame Sol Talisman and Essence Channeler, Nettlecyst
reprints, and early core sets such as Revised (`3ed`) and Unlimited (`2ed`) are searchable
even when another printing supplies the embedding. Placeholder-scan siblings are included
when that illustration has a usable scan elsewhere; artworks with no usable scan remain
absent. Each illustration still has one embedding/crop. Existing IDs and splits are kept;
duplicate legacy rows are retained as aliases in training metadata but not embedded twice.
Exact-printing corrections map back to the shared artwork for training and evaluation.
The separate deck printing picker remains English-only.

Room entry does not create a recognition worker or fetch models. The first card click or
gallery search starts `useRecognizer`: it fetches `GET /api/cardid/bundle`, creates the worker,
loads the three graphs plus compact `arts.json`, and warms up. Concurrent actions share this
load. The first action waits, displaying "loading card scanner…" or "Loading gallery…";
only then does the two-second inference timeout begin. Settings > Card scan shows "Not loaded"
until scanning starts, then `checking`, `loading`, `ready` (version, artwork count and load
time), `unavailable`, or `failed`. Leaving the room terminates the worker and pending actions.

New exports put sibling printing records in the separate, versioned `printings.json` file.
Only the first gallery search or expansion of a candidate's printing choices downloads it;
ordinary image identification needs only `arts.json`. The optional file is shared between
search/expansion requests and cached by bundle version. Expanding displays loading/retry
states, and choosing a sibling preserves its exact face, language, set and collector number.
Old bundles (embedded siblings or representative-only) still work. Re-export/publish a new
version on the training box to get the split; see Oracle's README. No room protocol changes.

Each capture runs identify with a two second timeout: detector pass over the 640 px crop, a
refine pass on the detected card, upright vote, embed all bundled art cuts (14 in new bundles;
older six-frame bundles still work), gallery search. A top-1
that leads the runner-up by at least `CLEAR_MARGIN` (0.08 cosine) is treated as the answer to a
plain click: it is recorded immediately and the **card preview** opens over the board — the
card image with its set and collector number, and (on wider screens) a box with mana cost, type
line, P/T or loyalty, and oracle text. That text is not in the catalog (which keeps one printing
per card), so `card-details.ts` asks `GET /api/card-printings/:id/details`, which
`Catalog.Printings.details/1` answers by fetching the exact printing from Scryfall once and
caching it in `card_printings`. "Wrong card?" on the preview reopens the picker for the same
crop and the chosen card replaces the entry.

For separate-side gallery entries, details select that face's name, image, mana cost, type
and rules text. The suffix is stripped only for the Scryfall request, not the details/query
cache key or correction label. The cached-printing `show` endpoint can also return either
face after details has populated it. Rulings use the base card's shared ruling list and
face-keyed cache rows. Malformed gallery IDs return HTTP 400 from details/rulings.

The **picker** (`card-suggestions.tsx`) is user-initiated only: it opens for a near-tie, when
no bundle is published (deck suggestions stand in), on "Wrong card?", or for an outlined card.
It shows five numbered candidates (`1`–`5`), the crop
with the detected quad and per-stage timings; low similarity never suppresses results. `/`
focuses a gallery search that understands names, set codes (`forest fin`, `set:fin`),
collector numbers (`#280`) and language (`lang:ja`). A bare word that is also a set code is
read both ways, so `woe strider` finds Woe Strider as well as any WOE "strider"; use `set:`
to force the set. English results sort first. Expand a
candidate's printing count to select its exact set/number/language; choices also show frame
effects, borderless treatment and promos. The Cards tab searches the same printing choices.
Ranks/keyboard `1`–`5` remain one per artwork, not one per reprint. Identical art cannot tell
printings apart automatically; the user chooses a sibling, and its own ID drives details,
hover images, rulings and correction labels. Name-based board deduplication is unchanged.

**Outlining a card** (`outline-drawing.ts`) is for when the detector misses, and to teach it:
Shift+click one corner of a card, then click the other three in any order (`Esc` or a click on
another board abandons it). The board shows the placed corners. The fourth click requests one
crop centred on the outline, which carries the corners in crop pixels (`CapturedCard.outline`,
mirrored by `orientCrop` with the image). The worker then skips the detector: it orders the
corners into a portrait quad, embeds both upright readings and keeps the one with the better
top match, so the returned quad starts at the printed top-left. The picker always opens, and
the chosen card uploads with that quad and `quad_source: "manual"`, which Oracle trusts as
detector ground truth. An outline too big for one 640 px crop is refused with a status message.

### Linked deck lists

When a seat's chosen deck links to Moxfield, Archidekt or ManaVault, every seat fetches the
list from `GET /api/decks/:id/decklist` (TanStack Query, 30-minute stale time) and preloads
its `small` then `normal` images, four at a time, so neither the list nor a later preview waits on
Scryfall. The server fetches the list live (five-minute cache) and joins it to the catalog; the
app stores no copy. Images come from the list's own printing when the site records one.

**Decks → View decklist** shows your own chosen deck's list, grouped like a deck builder, with
a list/images toggle, a filter, and a check on cards already identified on your board.
Opponents' lists are loaded only for recognition and are not shown, though anyone watching the
network tab can read them.

The clicked board owner's list nudges recognition after the bundle answers
(`deck-hint.ts`): each top-5 candidate whose name, or either face name, is in the list gets
`DECK_PRIOR` (0.03) added before re-ranking. The clear-answer rule, keys `1`–`5` and the picker
all see the hinted order, and the picker marks those candidates "In deck". Gallery search from
the picker lists deck cards first. A deck card outside the recognizer's top five is not
recovered: a gallery-wide bias would need a new search-graph input and a new bundle. When a
recognized art holds the list's exact printing, found once per list by the worker's `locate`
request, that printing is recorded. A printing picked by hand is never replaced. Corrections
keep the raw result, so training data is not biased by the hint. Oracle's README, "Deck-list
prior", describes how to check the prior against real captures.

Hover or keyboard-focus a candidate (including gallery search results) to see a larger card
image beside the picker before choosing. The same hover preview shows a seat's commander in
the rail, board name bar, turn-order table, commander picker, and Decks tab. Commander hover
uses the deck serializer's `commander_image_url` (selected printing, or catalog default),
falling back to `commander_art_crop_url` without a Scryfall details request.

The card popup keeps **Wrong card?**, **Remove**, **Rulings**, and close together in a dark,
consistently sized toolbar. Clicking outside the visible content, including the space below
the rules text, closes it. Right-click its content or use **Rulings** to view Scryfall rulings;
the popup is the only place rulings are offered (tray entries just open the popup). The authenticated
`GET /api/card-printings/:id/rulings` endpoint fetches `/cards/:id/rulings` with Req and caches
successful results (including an empty list) in `card_rulings_cache` for one day. Errors are
not cached, and the dialog offers retry. This cache is independent of printing/catalog refreshes.

Identified cards are not game events and never appear in the Log. Confirming a card (the silent
clear match, `1`–`5`, a click, or a search result) broadcasts `card_identified` on the data
channels and every seat adds the entry to that board's **card tray** (`board-cards.tsx`): a
chevron tab at the bottom of the active board's video that unfolds a translucent shelf of card
thumbnails, each with a red × to remove it (`card_removed`, honoured at every seat) and opening
the preview when clicked. The **Cards** tab of the side panel (`cards-tab.tsx`) shows the
newest identified card with its details (Clear hides it locally), a gallery search that
previews any printing, and the detected cards grouped per player with a Shared / My board
toggle. The list is persisted through the channel's `cards` event (up to 500 entries per room)
and synchronized through `identified_cards` and `table_state`. The server stamps each entry's
`byPlayerName` from the sender's seat (any client-supplied name is ignored), and every
`identified_cards` broadcast carries the change `type` and the acting seat as
`by: {peer_id, player_name}`. Any seated player may remove any entry to correct a
misidentification; spectators cannot change the list. Data-channel messages remain for
older clients; private reveal identifications are never persisted. If the card name matches one of the owner's
commanders and they have no deck selected yet, it also selects that deck.

Only a board's owner can wipe it: the tray on your own board and your own group in the Cards
tab offer **Clear cards**, which broadcasts `cards_cleared` with your peer ID and empties that
board at every seat (other boards keep their entries). Starting a game also clears every board:
each seat watches the shared timer and drops all cards on the lobby-to-started transition, so
cards identified while waiting never carry into the game. A late joiner's first timer sample is
already started and does not count as a transition, so the `cards_sync` it receives survives.

Each board keeps **one entry per displayed card name**, trimmed and case-insensitive (the gallery
does not provide oracle IDs). Different printings of the same card do not create extra entries.
Separate face names remain distinct; full combined prepare/adventure names stay intact.
Repeat identification previews the **printing just clicked** while preserving the tray entry's
first printing and position. Remove and correction still target that existing entry;
another player's board can still hold its own entry. Picker choices use the same
rule. "Wrong card?" removes the mistaken entry, then reuses an existing replacement if present.
Incoming entries and late-join syncs also deduplicate; simultaneous discoveries choose the oldest
timestamp, then entry ID, so message arrival order does not decide which printing survives.

The popup's **printing arrows** wrap around all pages of the existing English paper printing
list, newest first. It starts on the clicked printing; if absent (for example, another language
or a face-specific gallery entry), that printing is prepended. The counter and set/collector
caption track the displayed printing; browsing never changes the tray. Neighbouring images are
prefetched (only the two neighbours, at low browser priority). Left/Right works only while the preview has focus, not while typing, using a keyboard
widget, or viewing rulings. Table shortcuts remain paused under the preview.

Preview and card hover show Scryfall USD prices: `$0.25 · Foil $1.10 · Etched $1.25`, omitting
unavailable finishes and showing `—` when none are priced. Printing details (including prices)
are fresh for one hour in TanStack Query and use `cache-control: private, max-age=3600` on the
server. The printing list also has a one-hour client cache. Loading/failure does not hide the
clicked card; failed printing lists can be retried.
Page requests are spaced by 500 ms to respect the existing Scryfall search limit. The list
excludes tokens and memorabilia, matching the catalog/details contract (Scryfall includes
memorabilia basic lands in printing searches).

All catalog card images now pass through the authenticated, same-origin image cache described
in the README. Thumbnails use `small`, previews `normal`, and art tiles `art_crop`; off-screen
images load lazily and decode asynchronously. The server shares concurrent misses across seats
and limits upstream image downloads to four at once. Disk storage is `DATA_DIR/card-images`,
bounded to 512 MiB with oldest-written eviction and 30-day expiry; browsers cache for one day
and can revalidate using ETags. No new environment variables, service worker, or image
transformations. `x-card-image-cache: hit|miss` distinguishes server disk reuse from downloads.

Backlog:

- record identified cards against the historical game (currently retained only with the room session);
- WebGPU execution provider with WASM fallback.

Explicit picker choices (including an explicitly confirmed top-1, but never an automatic
answer) POST their native JPEG, click, quad/up vote, chosen gallery ID and original ranking
metadata to `/api/cardid/corrections`; an outlined card adds `quad_source: "manual"` (the
server accepts `detector` or `manual`, and `manual` only with a quad). The small checkbox below the active board opts out in
localStorage; the camera owner's preference also travels with each crop. The save note only
appears after server acknowledgement and failures never interrupt identification.

Phoenix stores the private, bounded samples under `DATA_DIR/cardid/corrections`. The offline
desktop importer creates the card warp and merges captures into Oracle's `data/real`; the NUC never
trains or runs Python. Admin-only export, filesystem import, manual training and optional
guarded nightly runs are documented in Oracle's README, **Training from in-app corrections**.
