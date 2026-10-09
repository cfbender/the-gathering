# Webcam rematch model checking

This is a bounded TLA+/TLC audit of **rematch isolation**, not a proof of the
application. Two counterexamples were reproduced against Rust and then fixed.
`Fixed = FALSE` preserves the original buggy behavior for comparison;
`Fixed = TRUE` models the production repairs. The Rust regressions now pass.

## Run TLC

Requires Java 17 or newer. Download the standalone TLC jar outside the repository:

```sh
curl -fL https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar \
  -o /tmp/tla2tools.jar
echo '7beec0f04818732a62fa193731711a99aa4f11279499b2360a7d156c519ea78d  /tmp/tla2tools.jar' \
  | sha256sum -c -
```

From `rust/models/`, run each configuration (change `check`):

```sh
check=fixed # baseline, structure, turn, seat, or fixed
java -XX:+UseParallelGC -cp /tmp/tla2tools.jar tlc2.TLC \
  -workers 1 -seed 1 -fp 0 -noGenerateSpecTE \
  -metadir "/tmp/tlc-webcam-$check" \
  -config "WebcamRematch-$check.cfg" WebcamRematch.tla
```

Observed with TLC `2026.10.06.014338` and OpenJDK 17:

| Configuration | Result | Distinct states | Trace / search depth |
| --- | --- | ---: | ---: |
| `baseline` (no rematches) | No invariant violations | 32 | 10 |
| `structure` (one rematch) | Types and active-player/phase consistency hold | 203 | 15 |
| `turn` | `NoCrossGamePass` violated, exit 12 | 45 before stopping | 6 states |
| `seat` | `ResetLifePreserved` violated, exit 12 | 38 before stopping | 5 states |
| `fixed` (two rematches) | All four invariants hold, exit 0 | 566 | 18 |

The first four configurations use `Fixed = FALSE`. The failure runs stop at
the first counterexample; they do not exhaust the graph.
The no-rematch baseline does not exercise cross-game safety and is only a
control. `structure` explores the complete bounded graph, including the bad
states, while checking only structural invariants. Deadlock checking is disabled:
this model checks safety, and reaching the artificial action bounds is allowed.

## Model-to-code mapping

Paths below are relative to `rust/crates/the-gathering/src/`.

| Model action | Implementation |
| --- | --- |
| `Start` | `webcam/room.rs`: `start_game` → `reorder` → `Entry::reconcile_turn`; `webcam/turns.rs`: `reconcile` → `pass` |
| `CapturePass` / `DeliverPass` | A delayed browser request; `web/channels/webcam_table.rs`: `handle_in("pass_turn")`; `webcam/room.rs`: `RoomMsg::PassTurn` |
| `Rematch` | `webcam/room.rs`: `rematch` resets turn accounting (but advances revision when fixed), resets seats, commits, and queues `ConnEvent::SeatReset` |
| `Damage` | A completed `update_status` handler and `remember_seat` round trip, collapsed into one action |
| `PrepareSeat` / `RememberSeat` | `Channel::update_status` copies its cached full seat to `WebcamTables::remember_seat`; the fixed room also checks the seat generation before accepting the snapshot |
| `ReceiveReset` | `Channel::handle_conn_event(SeatReset)` replaces the channel cache and presence, not the durable room seat |

The channel's `tokio::select!` is biased toward room events. `PrepareSeat` is
therefore disabled while a reset is pending. However, an already-running
handler awaits the room reply without polling that select. `ReceiveReset` is
disabled until the in-flight write finishes. Alice's rematch and Bob's update
originate from different channels; this does not assume messages reorder on one
socket or in the room's FIFO inbox.

Bounds and abstractions: two connected, non-eliminated Commander players in a
fixed order; up to two rematches and revisions 0–8 for `fixed` (one rematch and
revisions 0–4 for the original behavior); one pending pass and one pending seat
write; life values 17 and 40. `epoch` is specification-only bookkeeping for turn
requests, and models `Seat.generation` in fixed seat writes. All writes succeed.
The model omits clocks, elimination, undo/count accounting, disconnects, authentication, crashes,
SQLite failure interleavings, other modes, and the SFU. There is no refinement
proof connecting this hand-written model to Rust and no liveness claim.

## Original counterexample 1: turn revisions are reused across games

```text
Start: revision 1, Alice active
CapturePass: request holds revision 1 from game 0
Rematch: revision 0, no active player
Start: revision 1, Alice active in game 1
DeliverPass: accepted; Bob becomes active in game 1
```

`RoomMsg::PassTurn` checks only revision equality and an active player. Resetting
the revision permits an old game's delayed request to advance the new game.
The original Socket.IO regression observed `("ok", Bob, {Alice: 1, Bob: 1})` where
the expected outcome is `("error", Alice, {Alice: 1})`.

**Repair:** rematch advances the current turn revision while clearing counts,
elapsed time, active player, and history. Revisions no longer repeat across
games in the same room; both pass and undo use this guard. The wire request
shape is unchanged, and a fresh pass/undo in the new game still works.

## Original counterexample 2: an in-flight seat write reverses the reset

```text
Damage: Bob's room seat and channel cache both have 17 life
PrepareSeat: a camera-only update captures the full old seat
Rematch: room saves 40 life and queues SeatReset for Bob
RememberSeat: same connection's old full seat saves 17 life again
```

Receiving `SeatReset` afterward updates only the channel cache/presence. The
room snapshot and persisted session still contain 17 life. This applies to
other reset counters carried in the full seat too, though the regression checks
life specifically.

The actor-level regression uses the real `WebcamTables` API and a disposable
SQLite database. `tokio::join!(biased; rematch, remember_seat)` deterministically
queues the rematch first, and holds the connection event receiver while the
write completes. It reproduces the room boundary directly; it is not a
probabilistic browser race test or an end-to-end execution of the channel loop.

**Repair:** `Seat::reset` increments a server-owned generation. The room rejects
full-seat writes from older generations, even from the current connection.
The handler returns `seat has changed; try again` and consumes its queued
`SeatReset` before handling another client event; stale operations are not
automatically replayed into a different game. Deck/reveal side effects and
presence publication happen only after an accepted write.

Status writes carry their optional elimination in the same room command. This
prevents a rematch between saving life and applying elimination from knocking
out a seat in the new game. Rust tests cover this; elimination is not modeled
in TLC. The generation is persisted with the seat; serde defaults it to zero
for existing saved sessions. Join input cannot choose the generation.

## Rust reproductions

From `rust/`:

```sh
mise exec -- cargo test --locked --test integration tlc_rematch -- --nocapture
```

Both tests live in `tests/integration/webcam_room_lifecycle.rs` under the server
crate. They exercise observable public behavior without production test hooks:

- `tlc_rematch_rejects_a_pass_captured_in_the_previous_game`: real Socket.IO
  acknowledgement, active player, turn counts, and a valid new pass/undo.
- `tlc_rematch_preserves_reset_life_when_an_in_flight_seat_update_arrives`:
  room actor API, reset notification, live snapshot, persisted session, stale
  elimination rejection, valid new updates, and another rematch after restart.

Before repair: **0 passed, 2 failed**. After repair, both regressions and all
**94 webcam-related tests pass** with
`mise exec -- cargo test --locked --test integration webcam_`.
Run the repository-wide gate from the repository root with `mise run precommit`.

The full gate passes: schema validation, Rust formatting/Clippy and all Rust
suites (including 613 integration tests), frontend checks and 489 tests, and
the production build. A two-seat portal smoke check also confirmed life resets
from 17 to 40, a camera toggle preserves 40, a new life update saves 29, and
pass/undo works after rematch. The deterministic Rust tests, rather than the
browser smoke check, exercise the stale-command interleavings.
