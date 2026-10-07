---
id: TASK-1.8
title: Replace the Phoenix Channels protocol and phoenix client for the webcam table
status: To Do
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:55'
labels: []
dependencies:
  - TASK-1.1
parent_task_id: TASK-1
priority: medium
type: enhancement
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The owner decided (2026-10-07) to replace the Phoenix Channels V2 protocol as part of TASK-1 rather than defer it (this supersedes DRAFT-1). The webcam table speaks Phoenix Channels V2 (`web/channels`: `[join_ref, ref, topic, event, payload]` frames, heartbeat, phx_join/phx_leave/phx_reply/phx_close/phx_error, a single-node `Phoenix.PubSub` with fastlaning, and Presence diffs with phx_ref metas) because the frontend uses the `phoenix` npm client (`features/webcam-table/use-room-channel.ts`, `room-link.ts`, `use-table-game-state.ts`, `use-board-cards.ts`, `use-seat-trackers.ts`, `test-support/fake-phoenix.ts`). Prefer a popular library pair over hand-written protocol code. Research at audit time: socketioxide (a Socket.IO server as a tower layer for axum, with rooms, acks, per-socket extensions, and state; about 27k downloads a month; MIT) paired with the official `socket.io-client` (reconnection with backoff, buffering, `emitWithAck` timeouts). Presence is not built into Socket.IO, so the server keeps its own room roster and emits it. If socketioxide cannot satisfy a requirement (frame size limits, auth before the handshake completes, per-connection rate limiting, revocation on logout), fall back to a small typed JSON protocol over axum WebSockets with `partysocket`/reconnecting-websocket on the client.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The webcam table connects through a library-backed realtime transport (socketioxide plus socket.io-client unless research rules it out); web/channels/protocol.rs, the Phoenix-style pubsub fastlane, and phx_* events are gone, and the phoenix and @types/phoenix packages are removed
- [ ] #2 Joining, leaving, seat signaling (offers, answers, ICE, restarts), game state, board cards, trackers, log, timer, and presence work for multiple seats; a crashed or ended room makes clients rejoin or close as before
- [ ] #3 Connection auth uses the native socket token; invalid, expired, or revoked tokens and logout or password change disconnect the socket (Rust tests)
- [ ] #4 Per-connection event and signal rate limits, join limits, and the inbound size cap still apply (tests)
- [ ] #5 Frontend tests replace fake-phoenix with a fake for the new client; a portal check with two browser seats shows video and table state syncing
- [ ] #6 mise run precommit passes
<!-- AC:END -->
