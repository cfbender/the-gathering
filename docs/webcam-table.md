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
to your own known commanders as deck-based suggestions (only on clicks over your own board, since
only a seat's owner may choose its commander).

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
  Other seats have read-only life and an always-visible chevron to inspect their counters.
  The ⋯ menu offers pin/unpin and eliminate/restore for every seat; your own menu also has
  camera on/off and Reveal hand, opening the existing private-reveal flow in a dialog.
  Commander names truncate when necessary; the full name remains in the title/hover preview.
  Empty seats up to ten render as dashed "Open seat" placeholders. The rail scrolls vertically
  on desktop and horizontally in 240px tiles at narrow widths rather than shrinking ten cameras until their
  names and controls are unreadable. Clicking a tile makes it the active board and pins it.
- **Commander identity** colors both the rail and active-board name bars: one muted solid for
  mono-color, a WUBRG-ordered gradient for multiple colors, and neutral for colorless or unknown.
  The active-board commander name shows identity pips using the existing mana symbols; compact
  rail bars omit pips and the YOU tag to preserve name space at minimum width. The source is the decks
  API's stored `color_identity` (including partners), not a browser Scryfall request. An empty
  identity is treated as unknown, not falsely labelled colorless; explicit `C` shows its pip.
- **Active board** (center): the stage fills the remaining viewport. The same life control sits
  top-left, a Pin/Pinned toggle top-right, and the matching name bar underneath carries the seat
  menu, camera state, and "Select commander" popover without repeating life. Unpinned, the stage follows the newest
  remote joiner; when the active player leaves it falls back to your own board. Clicking the
  video starts the click-to-identify flow, and the suggestion card floats bottom-center over the
  stage (keys 1–5 still pick).
- **Side panel** (right): a narrow icon strip (Table, Decks, Cards, Log, Settings) plus a collapse chevron and shortcut help. The
  Table tab holds the Setup section (elapsed-time badge and players count in the header, Invite
  players copies the room URL, Select your commander, a turn-order table with #/Player/Turn/Time
  (life and commander under the name), then before the match an auto-randomize toggle and the
  primary Start match / Randomize and start button, and after it Pass turn and Pause/Resume
  timer; Reveal hand to…, the red End game button, Leave table), dice/coin controls, and collapsed Identify
  cards and Connection sections. Decks lists your commanders; Log shows the table event log. Collapsing the
  panel leaves only the icon strip so the board grows.
- **Resize dividers** on desktop drag the camera rail (176–360 px, default 240) and panel content
  (240–480 px, default 288); widths also cap at 24vw / 32vw to preserve board space. Double-click
  resets one divider. Focus a divider and use Left/Right to resize by 16 px, Home to reset.
  Settings offers a reset for both widths and a hotkeys toggle. Preferences persist per player
  in this browser under `the-gathering:table-preferences:<playerId>`; they are not room state.
- **Keyboard shortcuts** (Convoke-compatible where the table has the same feature):
  `Space` passes the turn after the match starts; `↑` / `↓` gains/loses one life and
  `Shift+↑` / `Shift+↓` gains/loses ten life (always your own seat). `[` / `]` subtracts/adds
  two commander tax by changing your primary commander's cast count by one; partner commanders
  retain individual counter rows. `C` toggles your camera, `B` collapses/expands the panel,
  `T` / `D` / `A` / `L` / `S` opens Table / Decks / Cards / Log / Settings, and `,` / `.`
  selects and pins the previous/next board. `?` or `H` toggles the grouped shortcut dialog.
  All table actions, including Space, honor the enable preference and pause while typing,
  using a keyboard widget, or while an overlay/picker is open. Ctrl/Alt/Meta, composing and
  repeat events are ignored; Space preserves native button/link activation. The card picker
  retains `1`–`5` and `/` gallery search. Escape closes overlays even with shortcuts disabled.
- **Settings** stacks collapsible Keyboard shortcuts, View, Camera, Sound and Card scan sections.
  View switches between selected/pinned boards and following the active turn; manually selecting
  or cycling a board stops following. Left/Right swaps the side panel and camera rail on desktop;
  narrow layouts keep cameras above and controls below. Glass/Classic uses the existing global
  theme-style preference. Camera lists available devices, remembers the choice and enabled state,
  and replaces outgoing tracks on existing peer connections without leaving the room. Camera-off state and
  private-reveal restrictions survive switching. Missing saved cameras fall back to the system
  default with a warning. Capture requests ideal 1080p (lower-resolution devices are accepted).
  Publisher quality offers Auto (the existing seat-count tiers), 1080p, 720p or 540p ceilings;
  it changes sender scaling/bitrate without lowering native card-crop resolution. Stats sample
  each connection every two seconds while enabled: remote tiles show received resolution, fps,
  bitrate and the selected remote ICE candidate type; the local tile shows native capture
  resolution/fps (no network hop). Check video health reports local track settings and state.
  Turn sound is an opt-in WebAudio tone, unlocked by interaction, on transitions to your turn.
  Card scan contains recognition bundle status/version and the existing corrections-sharing
  opt-out (`the-gathering:share-card-corrections`). Other table preferences use the per-player
  browser key above; sound, stats and follow-turn start off. There is no microphone, hand-count
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
life is also tracked locally so rapid ± clicks compound before presence echoes back, and it is
republished after every (re)join because presence restarts at the defaults.

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
deltas and are republished on channel rejoin like life; a full page reload resets the seat.

**Take the monarch** claims the crown for your own seat. `take_monarch` has an empty payload;
`WebcamTableMonarch` serializes claims and broadcasts one `monarch` holder, never per-seat flags.
Late joiners receive `monarch_state`; server revisions prevent stale snapshots from replacing
newer claims. When the holder disconnects, the crown is cleared rather
than reverting to a previous holder. A crown appears on the holder's tile and active board.
Monarch state is in memory only, scoped by room, and resets when the application restarts.
Counter changes and monarch transfers are added to every connected browser's Log.

Any seated player can mark a present seat eliminated or undo it in the turn-order table.
`set_eliminated` validates a present peer ID and a boolean; the target channel merges it into
its own presence so later life/camera updates cannot overwrite elimination. Players may also
publish their own `eliminated` through `update_status`. Out seats stay visible with dimmed video,
an Eliminated badge, and a struck-through name. They receive no order number; only eligible seats
are numbered, while the recording order still includes everyone. Eliminating or disconnecting
the current player advances the turn to the next eligible seat. If none remain there is no active
turn; restoring a player starts their next turn. Counts and accumulated time are kept.

Eliminated seats are retained in `WebcamTableState` and shared through `eliminated_seats`, so
leaving does not drop them from the result or from a late joiner's view. Rejoining as the same
player takes back the retained position and elimination flag. A departed seat must rejoin before
it can be restored. State still clears when the last connected seat leaves. The End game form
suggests the only non-eliminated player as winner (when at least two players took part), with all
others defaulting to losses in recorded seat order. The result remains editable; choosing Draw
explicitly records the whole table as draws, as required by the existing game schema.

Default turn order is join order (`joined_at`, then peer id) so every browser agrees.
"Randomize and start" sends `start_game`; the server shuffles the present peers unless the shared
`turn_settings` option `auto_randomize` is false. With that option off the button reads "Start
match" and preserves join order. Subsequent randomizations push `seat_order`; the channel verifies
the list names exactly the present peers, then broadcasts it. Rows shuffle visibly for about one
second before settling into that order (instant with reduced motion, no animation for an ordered
start). The End game form numbers seats in that order and records them the same way. Randomizing
again reorders seats but **never resets or resumes the timer or changes the current turn**.

Any seat can use **Pass turn** or **Space**. Turns advance in the shared order, skip eliminated and
departed seats, and wrap around. TURN counts increment when a turn starts, including the first
turn; small −/+ controls send `adjust_turn` corrections (0–999) without changing time or active
player. `pass_turn` requires the last seen turn revision, so simultaneous passes only advance
once. TIME is the player's accumulated time in m:ss, including their current turn. The server
banks time against game elapsed time rather than wall time, so pausing freezes both game and
player clocks; passing while paused changes the turn but adds no paused time. A single remaining
player can continue taking turns; elimination does not automatically end the game.

An amber dot and highlight mark the current player's row; an amber Current turn badge marks their
rail tile and board. This is independent of the violet selected/pinned-board border. Space does
not pass while typing, using a control, holding a modifier, repeating a key, composing text, or
while a card picker/dialog is open. It shares the guarded registry and enable preference in
`table-hotkeys.tsx` with the other table shortcuts.

`WebcamTableState` serializes the shared timer, turns and order on the single application server.
The first valid `seat_order` starts it. Any seat can send `timer` with `pause` or `resume`;
only the server writes `started_at`, `paused_at`, and accumulated `paused_ms`. The timer bar
above the active board's name bar derives elapsed time excluding pauses. Browsers interpolate
from a server sample using `performance.now()`, not their wall clock, and resync every 15 seconds
with `timer_sync` (half-round-trip latency compensation). New/rejoining seats receive the
current timer, order, settings, counts and per-player times via `table_state`. Channel monitors discard state when the last seat
leaves; an application restart also clears it. Multi-node room state is not supported.

End game pauses the timer for everyone and captures that server response for the result form.
Duration is editable, prefilled in whole minutes rounded to nearest (minimum one minute, matching
the game schema). Without a started timer it stays blank. Going back leaves the timer paused;
any seat can resume explicitly. `played_at` uses the shared start timestamp when available.

The Table tab offers d6, d20, custom dice with 2–1000 integer sides, and coin flips. The `roll`
event validates the request, generates the result on the server, stamps it with the authenticated
seat's name/id and server time, then broadcasts to everyone. Results appear in a five-second
overlay and in the Log. Clients cannot supply a result or impersonate the roller.

The Log tab is client-side only: it is derived from presence joins/leaves/changes and the
`seat_order` and `roll` broadcasts, capped at 200 lines, and not persisted or replayed on join.
Consecutive events of the same kind and actor within two seconds coalesce: life keeps the first
and final totals, dice/coins retain every result, and deck/camera changes show the latest state
with a count. Different actors, event kinds, and intervening entries break the group.

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
  merges `update_status`/`set_eliminated` into presence, validates `seat_order`/`timer`/`timer_sync`,
  `start_game`/`turn_settings`/`pass_turn`/`adjust_turn`, and generates
  and broadcasts validated `roll` results.
- `TheGatheringWeb.WebcamTableState` owns serialized, server-stamped timer/turn/order state and
  retained eliminated seats and channel-monitor cleanup (covered by `webcam_table_channel_test.exs`).
- `TheGatheringWeb.WebcamTableTurns` owns pure turn advancement, elimination skipping, counts and
  accumulated-time accounting (`webcam_table_turns_test.exs`).
- `TheGatheringWeb.Presence` owns ephemeral room membership and seat status.
- `WebcamTableConfigController` exposes authenticated ICE configuration.
- `features/webcam-table/use-webcam-room.ts` owns camera, mesh, signaling, native crop RPC,
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
- WebGPU execution provider with WASM fallback;
- shift-click manual four-corner capture when the detector misses.

Explicit picker choices (including an explicitly confirmed top-1, but never an automatic
answer) POST their native JPEG, click, quad/up vote, chosen gallery ID and original ranking
metadata to `/api/cardid/corrections`. The small checkbox below the active board opts out in
localStorage; the camera owner's preference also travels with each crop. The save note only
appears after server acknowledgement and failures never interrupt identification.

Phoenix stores the private, bounded samples under `DATA_DIR/cardid/corrections`. The offline
desktop importer creates the card warp and merges captures into `ml/data/real`; the NUC never
trains or runs Python. Admin-only export, filesystem import, manual training and optional
guarded nightly runs are documented in `ml/README.md`, **Training from in-app corrections**.
