# Webcam table

## Product slice

A webcam table is a temporary room for two to four signed-in members. Each member is seated as
the player linked to their account, chooses a deck while in the room, shares their board camera,
and ends the game with a result form that records through `Games.create_game/2`. The resulting
`Game` and `GamePlayer` rows are deliberately
indistinguishable from a manually logged game, so existing history and statistics need no
special cases.

Card clicks assist the room rather than define its durable record. A click on any board fetches
a native-resolution crop from the camera owner's browser, runs the card recognizer in the
clicking browser, and shows five numbered candidates plus a gallery search; confirming one posts
an "identified" line to every seat's log. The recognizer bundle is published to the server from
`ml/` (see **Recognition** and `ml/README.md`); when the server has none, the panel falls back
to that player's known commanders as deck-based suggestions.

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
in the mesh case. Host candidates alone only work on one LAN: two browsers on different networks
each sit at "Connecting…" forever because neither learns a reachable address. `WEBRTC_STUN_URLS`
therefore defaults to public STUN servers (Google and Cloudflare); override it with your own or
set it to `none` for a LAN-only install. STUN gets through ordinary home routers; players behind
symmetric NAT or carrier-grade NAT also need a separately reachable TURN server configured with
`WEBRTC_TURN_URLS`, `WEBRTC_TURN_USERNAME`, and `WEBRTC_TURN_CREDENTIAL`. When a peer connection
fails the offering side restarts ICE once; a peer that stays failed is labelled "Couldn't
connect" on its tile and the Connection section points at TURN. TURN cannot be hidden behind an ordinary HTTP reverse proxy: expose its
UDP/TCP listener (commonly 3478) and preferably TURN-over-TLS (commonly 5349/443) from coturn or
another relay. Credentials are returned only from the authenticated config endpoint.

The hosted alternative is Cloudflare Realtime TURN: set `CLOUDFLARE_TURN_KEY_ID` and
`CLOUDFLARE_TURN_API_TOKEN` (create the key under Realtime → TURN in the Cloudflare dashboard) and
`TheGathering.CloudflareTurn` exchanges that long-lived key for per-join credentials via
`POST https://rtc.live.cloudflare.com/v1/turn/keys/:id/credentials/generate-ice-servers`. The
credentials expire after `CLOUDFLARE_TURN_TTL_SECONDS` (default six hours, longer than a game;
refreshing mid-session would need `RTCPeerConnection.setConfiguration`). The config endpoint
appends Cloudflare's servers after the static ones, dropping URLs the static list already covers,
and falls back to the static list with a logged warning if Cloudflare is unreachable, so a
Cloudflare outage degrades to STUN-only rather than blocking the room. Only pairs that cannot
connect directly use the relay; Cloudflare bills relayed egress at $0.05/GB after the first
1,000 GB each month (STUN at `stun.cloudflare.com` is free and unlimited). One relayed 1080p
player in a four-seat, three-hour game is roughly 20 GB.

### Browser inference in the clicking browser

The detector, perspective warp, art crop, embedder, and cosine search run in the browser of the
player who clicked, on the crop the camera owner already returns over the data channel (see the
next section). Running it there rather than in the owner's browser costs nothing extra: the crop
transfer already solves the resolution problem, each browser loads the bundle once, and the
result needs no second round trip before it can be shown, corrected, and announced. ONNX Runtime
Web (`onnxruntime-web`) executes the three exported graphs on its single-threaded WASM backend;
the glue around them (`recognition/pipeline.ts`) is a line-for-line port of the Python
`cardid.bundle` reference runtime, and the two give the same top five on rendered scenes. WebGPU
is a later optimisation. References:

- <https://onnxruntime.ai/docs/tutorials/web/>
- <https://onnxruntime.ai/docs/tutorials/web/ep-webgpu.html>

Do not run inference in Phoenix: Ortex/Nx would make the application host the hot compute path,
complicate CPU portability, and upload imagery. A Python sidecar could remain in one container,
but would add a second supervised runtime and duplicate the spike runtime in production.

### Remote clicks use the source camera's native frame

`getUserMedia` requests a hard minimum of 1920 × 1080. A click on a remote tile is sent as
normalized coordinates over that pair's WebRTC data channel. The camera owner's browser maps the
coordinates to its native `videoWidth`/`videoHeight`, captures the same 640 px JPEG crop used by
`cardid.capture`, and returns it on the data channel together with the click position inside the
crop. The requester recognizes the card from that crop.

This protocol does not depend on the resolution selected by WebRTC congestion control (a
960 × 540 received stream still yields a crop of the owner's 1920 × 1080 frame) and keeps the
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

The Games page still finds live tables without the URL: every seated channel process also
tracks itself on one lobby presence topic (`TheGatheringWeb.WebcamTableRooms`), and
`GET /api/webcam-table/rooms` groups that topic by room (players in join order, `full` at four
seats). `PlayActions` (`features/webcam-table/play-actions.tsx`) polls it every 15 s: with no
live table the header shows **Play**; with one it shows **Join** naming the seated players plus
a smaller **New table**; with several, Join becomes a menu of tables. Nothing is stored, so a
room vanishes from the list as soon as its last seat leaves.

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
`update_status` event (validated: life −999…999, camera boolean, and counters below; no other keys) and the channel merges
them into presence, so every browser shows the same totals without another round trip. Your own
life is also tracked locally so rapid ± clicks compound before presence echoes back, and it is
republished after every (re)join because presence restarts at the defaults.

The shield button in each seat bar opens **Counters**. Everyone can inspect a seat; only its
owner can change its counters. `poison` and `rad` start at zero. `commander_casts` maps commander
names to command-zone cast counts and displays the next tax as twice the count. Both commander
and partner/background names come from the selected deck. `commander_damage` maps opposing
player IDs to commander-name/count maps, keeping identical commanders at different seats separate.
Recorded damage stays visible when a source changes deck or leaves. Damage does not adjust life
automatically. Poison at 10 and damage of 21 from any one commander are flagged in red; damage
from separate commanders is never combined for the threshold. The server accepts only integers
0…999, maps of at most 100 entries, and names of 1…300 bytes. Counter changes use optimistic local
deltas and are republished on channel rejoin like life; a full page reload resets the seat.

**Take the monarch** claims the crown for your own seat. `take_monarch` has an empty payload;
`WebcamTableMonarch` serializes claims and broadcasts one `monarch` holder, never per-seat flags.
Late joiners receive `monarch_state`; server revisions prevent stale snapshots from replacing
newer claims. When the holder disconnects, the crown is cleared rather
than reverting to a previous holder. A crown appears on the holder's tile and active board.
Monarch state is in memory only, scoped by room, and resets when the application restarts.
Counter changes and monarch transfers are added to every connected browser's Log.

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
- `features/webcam-table/card-suggestions.tsx` — the click-to-identify overlay: crop with the
  detected quad, five numbered candidates, gallery search, timings.
- `features/webcam-table/recognition/` — `use-recognizer.ts` (hook owning the worker and its
  checking/loading/ready/unavailable/failed state), `recognizer.worker.ts` (ONNX Runtime Web
  sessions, warm-up, identify and search), `pipeline.ts` (pure port of `ml/cardid/bundle.py`:
  window resample, detector refine pass, upright vote, gallery search parsing; unit-tested in
  `pipeline.test.ts`), and `messages.ts` (worker protocol types).
- `TheGathering.CardId` + `CardIdBundleController` serve the published bundle from
  `DATA_DIR/cardid/current` (`GET /api/cardid/bundle` for the manifest and file URLs,
  `GET /api/cardid/bundles/:version/:name` for the immutable files). `404` means no bundle is
  published and the UI falls back to deck suggestions.
- `features/webcam-table/side-panel.tsx` — icon strip and Table/Decks/Cards/Log tabs
  (`cards-tab.tsx` holds the Cards tab; `panel-section.tsx` the collapsible section).
- `features/webcam-table/finish-game.tsx` — the End game result dialog.
- `features/webcam-table/table-events.ts` — pure helpers for log lines, seat ordering, and
  shuffling (unit-tested in `table-events.test.ts`).
- `features/webcam-table/board-cards.tsx` — the card tray docked to the bottom of the active board
  and the shared `CardThumb`.
- `features/webcam-table/card-preview.tsx` — the card image + rules text overlay; `card-details.ts`
  fetches one printing's details from `GET /api/card-printings/:id/details`.
- `routes/table.new.tsx` and `routes/table.$roomId.tsx` are thin route adapters.

The finish mutation posts the normal game payload (`played_at`, optional duration/turns/win
condition/notes, and consecutive seats with player/deck/result) to `/api/games`. The controller
already strips provenance fields and delegates to `Games.RecordGame`.

## Recognition

The recognizer ships as a **bundle** exported and published from `ml/` (`cardid.export`,
`cardid.publish`; see `ml/README.md`, "Shipping"). Phoenix serves whatever
`DATA_DIR/cardid/current` points at; nothing model-related is committed to this repository or
baked into the container image, so a new bundle (new model, or the same model with a refreshed
gallery after a set release) is a `publish` away and browsers pick it up on their next table
because they cache bundle files by version.

In the browser, `useRecognizer` fetches `GET /api/cardid/bundle` once per table, starts a Web
Worker, loads the three graphs plus `arts.json`, and runs one warm-up identify so the first real
click is not slow. The "Identify cards" section of the side panel shows `checking`, `loading`,
`ready` (with gallery size and load time), `unavailable` (no bundle published) or `failed`.

Each capture runs identify with a two second timeout: detector pass over the 640 px crop, a
refine pass on the detected card, upright vote, embed all six art cuts, gallery search. A top-1
that leads the runner-up by at least `CLEAR_MARGIN` (0.08 cosine) is treated as the answer to a
plain click: it is recorded immediately and the **card preview** opens over the board — the
card image with its set and collector number, and (on wider screens) a box with mana cost, type
line, P/T or loyalty, and oracle text. That text is not in the catalog (which keeps one printing
per card), so `card-details.ts` asks `GET /api/card-printings/:id/details`, which
`Catalog.Printings.details/1` answers by fetching the exact printing from Scryfall once and
caching it in `card_printings`. "Wrong card?" on the preview reopens the picker for the same
crop and the chosen card replaces the entry.

The **picker** (`card-suggestions.tsx`) is user-initiated only: it opens for a near-tie, when
no bundle is published (deck suggestions stand in), on "Wrong card?", or when the clicker
Shift+clicks to choose for themselves. It shows five numbered candidates (`1`–`5`), the crop
with the detected quad and per-stage timings; low similarity never suppresses results. `/`
focuses a gallery search that understands names, set codes (`forest fin`, `set:fin`) and
collector numbers (`#280`) so basics and staples with hundreds of printings can be narrowed.

Identified cards are not game events and never appear in the Log. Confirming a card (the silent
clear match, `1`–`5`, a click, or a search result) broadcasts `card_identified` on the data
channels and every seat adds the entry to that board's **card tray** (`board-cards.tsx`): a
chevron tab at the bottom of the active board's video that unfolds a translucent shelf of card
thumbnails, each with a red × to remove it (`card_removed`, honoured at every seat) and opening
the preview when clicked. The **Cards** tab of the side panel (`cards-tab.tsx`) shows the
newest identified card with its details (Clear hides it locally), a gallery search that
previews any printing, and the detected cards grouped per player with a Shared / My board
toggle. The list is ephemeral like the Log, but a seat that connects later receives the current
entries (`cards_sync`) when its data channel opens. If the card name matches one of the owner's
commanders and they have no deck selected yet, it also selects that deck.

Backlog:

- record identified cards against the game (currently only in the ephemeral per-board list);
- record corrections (a confirmed candidate that was not top-1) as labelled captures for the
  real-capture training set in `ml/data/real/`;
- WebGPU execution provider with WASM fallback;
- shift-click manual four-corner capture when the detector misses.
