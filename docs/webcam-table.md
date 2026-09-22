# Webcam table

## Product slice

A webcam table is a temporary room for two to four signed-in members. Each member is seated as
the player linked to their account, chooses a deck while in the room, shares their board camera,
and ends the game with a result form that records through `Games.create_game/2`. The resulting
`Game` and `GamePlayer` rows are deliberately
indistinguishable from a manually logged game, so existing history and statistics need no
special cases.

Card clicks assist the room rather than define its durable record. The first slice captures a
native-resolution crop from the camera owner's browser and offers that player's known commanders
as correctable deck suggestions. The production recognizer boundary is in place, but model and
gallery artifacts are not committed yet; see **Recognition rollout**.

## Decisions

### Four-peer mesh now, SFU seam later

Use one `RTCPeerConnection` per pair and Phoenix Channels only for authenticated presence,
SDP, and ICE signaling. At the fixed maximum of four, a room has six connections and each
browser sends at most three streams. This is the smallest one-container architecture and keeps
media end-to-end between browsers. `ex_webrtc` implements a peer connection, not an SFU by
itself; a Membrane RTC engine would add server-side media routing, UDP port exposure, process
supervision, and materially more deployment work.

The cost is three 1080p encodes/uploads per player at room capacity. Measure sender CPU,
available outgoing bitrate, frame dimensions, packet loss, and TURN use. Move the implementation
behind the existing room hook to an SFU when real tables cannot sustain three 1080p sends. Room
and result APIs do not depend on the topology.

Phoenix's own Channels documentation confirms that signaling is application-defined and that
signed-token authentication belongs in `connect`; the authenticated config endpoint signs the
existing tracked session token for the socket handshake. MDN documents SDP and ICE exchange
through such a signaling service. References:

- <https://hexdocs.pm/phoenix/channels.html>
- <https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Signaling_and_video_calling>
- <https://github.com/elixir-webrtc/ex_webrtc>

### ICE, STUN, TURN, and proxies

The normal HTTPS reverse proxy must pass WebSocket upgrades for `/socket`; it never carries RTP
in the mesh case. Host candidates work on one LAN. `WEBRTC_STUN_URLS` supplies comma-separated
STUN URLs for NAT discovery. Reliable internet use also requires a separately reachable TURN
server configured with `WEBRTC_TURN_URLS`, `WEBRTC_TURN_USERNAME`, and
`WEBRTC_TURN_CREDENTIAL`. TURN cannot be hidden behind an ordinary HTTP reverse proxy: expose its
UDP/TCP listener (commonly 3478) and preferably TURN-over-TLS (commonly 5349/443) from coturn or
another relay. Credentials are returned only from the authenticated config endpoint. A static
credential is acceptable for a self-hosted first release; time-limited TURN credentials are the
follow-up for internet-exposed installations.

### Browser inference, with WASM fallback

Run the detector, perspective warp, art crop, embedder, and cosine search in the camera owner's
browser. ONNX Runtime Web supports WebGPU in current Chromium and WASM with full ONNX operator
coverage; WASM is the compatibility fallback. This avoids uploading board images, scales with
players, and follows the “browser where possible” constraint. The normalized 49k × 128 gallery
is about 25 MB as float32 (about 6 MB int8) and should be a versioned static binary loaded once by
a Web Worker alongside a small card-ID/name manifest. References:

- <https://onnxruntime.ai/docs/tutorials/web/>
- <https://onnxruntime.ai/docs/tutorials/web/ep-webgpu.html>

Do not run inference in Phoenix: Ortex/Nx would make the application host the hot compute path,
complicate CPU portability, and upload imagery. A Python sidecar could remain in one container,
but would add a second supervised runtime and duplicate the spike runtime in production.

### Remote clicks use the source camera's native frame

`getUserMedia` requests a hard minimum of 1920 × 1080. A click on a remote tile is sent as
normalized coordinates over that pair's WebRTC data channel. The camera owner's browser maps the
coordinates to its native `videoWidth`/`videoHeight`, captures the same 640 px JPEG crop used by
`cardid.capture`, and returns it on the data channel. Production recognition runs there and sends
only candidates back; the current artifact-less slice returns the crop so the requester can use
the deck suggestion UI. Shift-click manual four-corner capture and detector-produced top five are
the next recognizer UI increment.

This protocol does not depend on the resolution selected by WebRTC congestion control and keeps
the click-to-candidate latency budget local: capture + detector + embedder + gallery search, with
no server image round trip.

## Lifecycle and ownership

```text
Games page Play button ───────────▶ /table/:roomId
                                         │
                             auto-seat linked player
                                         │
                              choose deck in the room
                                         │
                            browser WebRTC mesh (≤ 4)
                                         │
                                 click End game
                                         │
                               complete result form
                                         │
                       POST /api/games → Games.create_game/2
                                         │
                              existing history + stats
```

Rooms are intentionally ephemeral and URL-addressed in this slice. Presence is the room roster;
refreshing rejoins, and an application restart drops signaling but no recorded game. Any signed-in
member with the unguessable UUID URL can join. Durable invitations/room recovery require a table
schema and are explicitly deferred.

## Table view layout

The room is laid out like a webcam play surface rather than a video-call grid: one board is
always large, everyone else is small, and controls live in a collapsible column.

```text
┌──────────┬─────────────────────────────────────────────┬──┬──────────────┐
│ rail     │ 40                                     Pin  │  │ Setup   3/4  │
│ ┌──────┐ │                                             │▪ │ Invite       │
│ │40    │ │                                             │▪ │ Commander    │
│ └──────┘ │              active board                   │▪ │ Turn order   │
│ Mara ♥40 │           (click = identify card)           │  │ Randomize    │
│ Select…  │                                             │  │ End game     │
│ ┌──────┐ │                                             │  │ Leave table  │
│ │37    │ │                                             │  ├──────────────┤
│ └──────┘ │                                             │  │ Identify  ▸  │
│ Cody ♥37 │                                             │  │ Connection ▸ │
│ Open seat├─────────────────────────────────────────────┤  │              │
│          │ Theo ♥40   −  +               📷  Select cmd │  │              │
└──────────┴─────────────────────────────────────────────┴──┴──────────────┘
```

- **Camera rail** (left, `lg:` 13 rem): every seat as a 16:9 tile with a life badge, a compact
  name bar (camera indicator, ± for your own seat), and that seat's commander action.
  Empty seats up to four render as dashed "Open seat" placeholders. Clicking a tile makes it the
  active board and pins it.
- **Active board** (center): the stage fills the remaining viewport. The large life badge sits
  top-left, a Pin/Pinned toggle top-right, and a name bar underneath carries life controls,
  the camera toggle, and the "Select commander" popover. Unpinned, the stage follows the newest
  remote joiner; when the active player leaves it falls back to your own board. Clicking the
  video starts the click-to-identify flow, and the suggestion card floats bottom-center over the
  stage (keys 1–5 still pick).
- **Side panel** (right): a narrow icon strip (Table, Decks, Log) plus a collapse chevron. The
  Table tab holds the Setup section (players count, Invite players copies the room URL, Select
  your commander, a turn-order table with #/Player/Commander/Life, the primary Randomize turn
  order button, the red End game button, Leave table) followed by collapsed Identify cards and
  Connection sections. Decks lists your commanders; Log shows the table event log. Collapsing the
  panel leaves only the icon strip so the board grows.
- The `/table/*` routes force the dark theme (`TableShell` in `routes/__root.tsx` swaps
  `data-theme` on mount and restores the user's choice on unmount) so portalled popovers and
  dialogs match the black stage. They render no application header.

### Shared seat state

Presence metadata carries, per seat, `life` (starts at 40), `camera_off`, and a
server-stamped `joined_at`. Players publish their own changes through the channel's
`update_status` event (validated: life −999…999, booleans, no other keys) and the channel merges
them into presence, so every browser shows the same totals without another round trip. Your own
life is also tracked locally so rapid ± clicks compound before presence echoes back, and it is
republished after every (re)join because presence restarts at the defaults.

Default turn order is join order (`joined_at`, then peer id) so every browser agrees.
"Randomize turn order" shuffles the present peers and pushes `seat_order`; the channel verifies
the list names exactly the present peers, then broadcasts it. The End game form numbers seats in
that order and records them the same way.

The Log tab is client-side only: it is derived from presence joins/leaves/changes and the
`seat_order` broadcast, capped at 200 lines, and not persisted.

Audio is not part of the webcam table: no microphone is captured and there are no mute
controls. Players use their usual voice app alongside the table.

## File and component structure

- `TheGatheringWeb.UserSocket` verifies a short-lived token wrapping the tracked cookie session.
- `TheGatheringWeb.WebcamTableChannel` caps rooms at four, relays targeted WebRTC signals,
  merges `update_status` into presence, and validates/broadcasts `seat_order`.
- `TheGatheringWeb.Presence` owns ephemeral room membership and seat status.
- `WebcamTableConfigController` exposes authenticated ICE configuration.
- `features/webcam-table/use-webcam-room.ts` owns camera, mesh, signaling, native crop RPC,
  seat status (life, camera), seat order, and the event log.
- `features/webcam-table/webcam-table-page.tsx` composes the rail, stage, and side panel and owns
  the active-board selection (`useActiveBoard`).
- `features/webcam-table/board.tsx` — `ActiveBoard`, `CameraTile`, `OpenSeat`, `LifeBadge`,
  and `capturePoint` (click → normalized coordinates).
- `features/webcam-table/seat-bar.tsx` — the name/life/camera bar under a board or tile.
- `features/webcam-table/commander-picker.tsx` — popover listing a player's decks; any seat can
  set another player's commander (the server still verifies deck ownership).
- `features/webcam-table/card-suggestions.tsx` — the click-to-identify overlay.
- `features/webcam-table/side-panel.tsx` — icon strip and Table/Decks/Log tabs.
- `features/webcam-table/finish-game.tsx` — the End game result dialog.
- `features/webcam-table/table-events.ts` — pure helpers for log lines, seat ordering, and
  shuffling (unit-tested in `table-events.test.ts`).
- `routes/table.new.tsx` and `routes/table.$roomId.tsx` are thin route adapters.

The finish mutation posts the normal game payload (`played_at`, optional duration/turns/win
condition/notes, and consecutive seats with player/deck/result) to `/api/games`. The controller
already strips provenance fields and delegates to `Games.RecordGame`.

## Recognition rollout

The repository currently contains neither exported ONNX files nor the production gallery. Before
claiming ML-backed identification, export/version these deployable artifacts from `ml/`:

1. detector ONNX and embedder ONNX;
2. normalized gallery binary plus ordered `{id,name,set}` manifest;
3. golden crop fixtures and expected top-five outputs shared by Python and browser tests;
4. a Web Worker implementing the two-pass 640→256 detector, upright perspective warp, art crop,
   ImageNet normalization, embedding, and top-five cosine search;
5. WebGPU/WASM latency telemetry and a hard one-second UI timeout that still opens name search.

The candidate panel must always show five numbered choices and `/` name search; low similarity
must not suppress results. Until those artifacts exist, the UI labels suggestions as deck-based
and does not imply that image recognition occurred.
