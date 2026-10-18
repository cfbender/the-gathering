---
id: TASK-10.2
title: Virtual board seats over the table channel
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-18 15:18'
updated_date: '2026-10-18 16:31'
labels: []
dependencies:
  - TASK-10.1
parent_task_id: TASK-10
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A seat can use a virtual board instead of a camera. The owner plays on the shared playtester full screen and can switch between it and the table; others see a read-only rendering of the public projection (hand and library as counts) synced over the Socket.IO table channel, not as video. Life, counters, and turns stay owned by the table. Needs the shared playtester package.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A seated player whose chosen deck has a linked decklist can switch their seat to a virtual board and back to the camera
- [x] #2 The owner plays on the shared @cfbender/playtest board full screen and can switch between it and the table view on phones and desktop
- [x] #3 Other players and spectators see a read-only rendering of the board in the stage, rail, grid, and phone layout, with hand and library shown only as counts
- [x] #4 Viewers can tap a visible card to preview it
- [x] #5 Life comes from the table seat, and the board untaps and draws when the table reaches that player's turn
- [x] #6 A late joiner, a reconnect, and a server restart all show the current board; a rematch deals a fresh game
- [x] #7 Board payloads from other clients are validated before rendering, and card images only load from the server's card image path
- [x] #8 Rust and frontend tests cover the board event and the client parser; precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Server: channel event `board` ({board: object|null}, seated only, 128 KiB cap) stored per player id in the durable room Entry (serde default), broadcast as `board`, sent once per join as `table_boards`, cleared on rematch and on removal. Integration tests.
2. Dependency: @cfbender/playtest from the v0.1.0 GitHub tarball (local tarball until the repo exists); import its styles.css in app.css.
3. Client room state: parse and keep boards by player id; publish own public projection.
4. Owner: useVirtualBoard keeps the full snapshot in localStorage per room and player, deals from the seat deck decklist, publishes projectPublicSnapshot on change and on (re)join, deals fresh on rematch, applies nextTurnSnapshot when the table turn reaches the player, turns the camera off while virtual.
5. UI: seat menu toggle; My board full-screen overlay (DeckPlaytester controlled, life from the table, back to table, pass turn); spectator board in place of video for virtual seats with card preview; My board in the phone thumb bar.
6. Verify in the portal with two players, phone and desktop.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented server board event (room Entry.boards, table_boards on join, cleared on rematch/removal) and client (virtual-board.ts parser/deal, use-virtual-board.ts, virtual-board-view.tsx, seat menu, phone thumb bar). Verified in the portal: phone owner deals from an Archidekt list, plays cards; desktop viewer sees board live with counts only; card preview opens; tap shows on viewer; life on board updates table; turn arrival untaps and draws; reload resumes; Use camera instead clears. rust:check (642 integration tests) and frontend precommit (541 tests) pass. Blocked on publishing: package.json still points at a local tarball of @cfbender/playtest until github.com/cfbender/playtest exists and v0.1.0 is tagged; then switch to https://github.com/cfbender/playtest/archive/refs/tags/v0.1.0.tar.gz, reinstall, and commit.

Switched to the published v0.1.0 tarball dependency (frozen-lockfile install works; precommit 541 tests pass; browser smoke test on the published package).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Seats can play on a virtual board from @cfbender/playtest instead of a camera. The owner keeps the full game locally and publishes its public projection over the new board channel event; the server stores boards in durable room state (table_boards on join, cleared on rematch/removal), and other seats render them read-only with card previews. Verified end to end in the portal (phone owner, desktop viewer), with rust:check and frontend precommit passing.
<!-- SECTION:FINAL_SUMMARY:END -->
