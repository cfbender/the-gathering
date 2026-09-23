# Webcam table

## Product slice

A webcam table is a temporary room for two to ten signed-in members. Each member is seated as
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

### Ten-seat adaptive mesh now, SFU seam later

Use one `RTCPeerConnection` per pair and Phoenix Channels only for authenticated presence,
SDP, and ICE signaling. At the maximum of ten, a room has 45 connections and each
browser sends at most nine streams. This retains the one-container architecture and keeps
media end-to-end between browsers. `ex_webrtc` implements a peer connection, not an SFU by
itself; a Membrane RTC engine would add server-side media routing, UDP port exposure, process
supervision, and materially more deployment work.

`media-policy.ts` budgets each outgoing sender by **total seated players, including self**:

| Seats | `scaleResolutionDownBy` | From a 1080p camera | `maxBitrate` per receiver |
| --- | --- | --- | --- |
| 1–4 | 1 | 1920 × 1080 | 2,500,000 bps |
| 5–7 | 1.5 | 1280 × 720 | 1,200,000 bps |
| 8–10 | 2 | 960 × 540 | 600,000 bps |

The hook applies these caps through `RTCRtpSender.setParameters` when a connection becomes live
and when membership changes (including restoring the higher tier after departures). Congestion
control may reduce quality further. At ten seats the video budget is at most 5.4 Mbps per
sender before transport overhead, still nine encodes and potentially nine TURN relays. Measure
sender CPU, available outgoing bitrate, frame dimensions, packet loss, and TURN use; these
tiers are budgets, not a guarantee of ten-seat performance on every device.

The SFU migration seam is `useWebcamRoom`: replace peer creation/signaling and stream delivery
behind that hook, preserving participant, stream, capture, reveal, and result APIs. The SFU
must enforce reveal subscriptions server-side and preserve the native crop RPC (a targeted data
channel or equivalent), rather than forwarding hidden video and relying on UI overlays. Move
there when measured CPU, uplink, or relay costs make the mesh impractical.

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
                            browser WebRTC mesh (≤ 10)
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
`GET /api/webcam-table/rooms` groups that topic by room (players in join order, `full` at ten
seats). `PlayActions` (`features/webcam-table/play-actions.tsx`) polls it every 15 s: with no
live table the header shows **Play**; with one it shows **Join** naming the seated players plus
a smaller **New table**; with several, Join becomes a menu of tables. Nothing is stored, so a
room vanishes from the list as soon as its last seat leaves.

## Table view layout

The room is laid out like a webcam play surface rather than a video-call grid: one board is
always large, everyone else is small, and controls live in a collapsible column.

```text
┌──────────┬─────────────────────────────────────────────┬──┬──────────────┐
│ rail     │ 40                                     Pin  │  │ Setup  3/10  │
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

- **Camera rail** (left, `lg:` defaults to 13 rem): every seat as a 16:9 tile with a life badge, a compact
  name bar (camera indicator, ± for your own seat), and that seat's commander action.
  Empty seats up to ten render as dashed "Open seat" placeholders. The rail scrolls vertically
  on desktop and horizontally at narrow widths rather than shrinking ten cameras until their
  names and controls are unreadable. Clicking a tile makes it the active board and pins it.
- **Commander identity** colors both the rail and active-board name bars: one muted solid for
  mono-color, a WUBRG-ordered gradient for multiple colors, and neutral for colorless or unknown.
  Commander names show identity pips using the existing mana symbols. The source is the decks
  API's stored `color_identity` (including partners), not a browser Scryfall request. An empty
  identity is treated as unknown, not falsely labelled colorless; explicit `C` shows its pip.
- **Active board** (center): the stage fills the remaining viewport. The large life badge sits
  top-left, a Pin/Pinned toggle top-right, and a name bar underneath carries life controls,
  the camera toggle, and the "Select commander" popover. Unpinned, the stage follows the newest
  remote joiner; when the active player leaves it falls back to your own board. Clicking the
  video starts the click-to-identify flow, and the suggestion card floats bottom-center over the
  stage (keys 1–5 still pick).
- **Side panel** (right): a narrow icon strip (Table, Decks, Cards, Log, Settings) plus a collapse chevron and shortcut help. The
  Table tab holds the Setup section (players count, Invite players copies the room URL, Select
  your commander, a turn-order table with #/Player/Commander/Life, the primary Randomize turn
  order button, the red End game button, Leave table) followed by collapsed Identify cards and
  Connection sections. Decks lists your commanders; Log shows the table event log. Collapsing the
  panel leaves only the icon strip so the board grows.
- **Resize dividers** on desktop drag the camera rail (176–360 px, default 208) and panel content
  (240–480 px, default 288); widths also cap at 24vw / 32vw to preserve board space. Double-click
  resets one divider. Focus a divider and use Left/Right to resize by 16 px, Home to reset.
  Settings offers a reset for both widths and a hotkeys toggle. Preferences persist per player
  in this browser under `the-gathering:table-preferences:<playerId>`; they are not room state.
- **Keyboard shortcuts**: `+` / `=` gains one life, `-` loses one life (always your own seat),
  `C` toggles your camera, `B` collapses/expands the panel, `T` / `D` / `A` / `L` / `S` opens
  Table / Decks / Cards / Log / Settings, `[` / `]` selects and pins the previous/next board,
  and `?` opens shortcut help. Shortcuts pause while typing, using a keyboard widget, or an
  overlay is open; modified, composing, and repeat key events are ignored. The card picker
  retains `1`–`5` and `/`. Escape closes overlays even with table shortcuts disabled.
  These are provisional bindings pending the Convoke reference; additional settings await that reference too.
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

### Private hand reveal

Choose another seated player in **Reveal hand to**, and wait for the confirmation before
showing your hand. Per-peer cloned video tracks are immediately disabled for non-targets, then
their senders use `replaceTrack(null)`; the native camera and target's sender stay live.
Late joiners also start with no outgoing video track. Presence carries `reveal_to`, validated
by the channel as another current peer, so hidden seats render **Revealing to name** instead of
a video element and the target sees **name is revealing to you**. The owner still sees their
own camera. Camera off overrides reveal and stays off when reveal ends.

**End reveal** restores eligible senders in one click. A target departure automatically ends
the reveal in both channel presence and the owner's media policy, restoring public video;
put the hand down before ending or leaving. Signaling reconnects preserve the local restriction.
The cap check and presence reservation are serialized so simultaneous final-seat joins cannot
overfill the room; duplicate peer IDs and duplicate players are refused.

Hidden viewers cannot identify the board: both the requesting UI and the owner's native-crop
handler enforce visibility. Authorized viewers still receive the owner's native 640 px JPEG
crop from the untouched 1080p source, unaffected by the sender tiers. Identifications during
a reveal stay local to the clicker rather than entering the shared card tray. A chosen viewer
can still save or share what they saw; this feature cannot revoke frames already delivered.

## File and component structure

- `TheGatheringWeb.UserSocket` verifies a short-lived token wrapping the tracked cookie session.
- `TheGatheringWeb.WebcamTableChannel` caps rooms at ten, relays targeted WebRTC signals,
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
`Game` accepts 2–10 distinct participants with consecutive seats; `GamePlayer` accepts seat
numbers 1–10 (and up to nine kills). The scrollable End game dialog and turn-order table both
render every participant in the shared order.

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

Hover or keyboard-focus a candidate (including gallery search results) to see a larger card
image beside the picker before choosing. The same hover preview shows a seat's commander in
the rail, board name bar, turn-order table, commander picker, and Decks tab. Commander hover
uses the deck serializer's `commander_image_url` (selected printing, or catalog default),
falling back to `commander_art_crop_url` without a Scryfall details request.

The card popup keeps **Wrong card?**, **Remove**, **Rulings**, and close together in a dark,
consistently sized toolbar. Clicking outside the visible content, including the space below
the rules text, closes it. Right-click its content or use **Rulings** to view Scryfall rulings;
tray entries also have a Rulings button and right-click shortcut. The authenticated
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
toggle. The list is ephemeral like the Log, but a seat that connects later receives the current
entries (`cards_sync`) when its data channel opens. If the card name matches one of the owner's
commanders and they have no deck selected yet, it also selects that deck.

Each board keeps **one entry per full card name**, trimmed and case-insensitive (the gallery
does not provide oracle IDs). Different printings of the same card do not create extra entries.
Repeat identification opens the existing entry's preview and preserves its first printing and
position; another player's board can still hold its own entry. Picker choices use the same
rule. "Wrong card?" removes the mistaken entry, then reuses an existing replacement if present.
Incoming entries and late-join syncs also deduplicate; simultaneous discoveries choose the oldest
timestamp, then entry ID, so message arrival order does not decide which printing survives.

Backlog:

- record identified cards against the game (currently only in the ephemeral per-board list);
- record corrections (a confirmed candidate that was not top-1) as labelled captures for the
  real-capture training set in `ml/data/real/`;
- WebGPU execution provider with WASM fallback;
- shift-click manual four-corner capture when the detector misses.
