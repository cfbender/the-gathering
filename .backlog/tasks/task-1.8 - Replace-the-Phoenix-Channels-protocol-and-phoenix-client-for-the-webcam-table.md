---
id: TASK-1.8
title: Replace the Phoenix Channels protocol and phoenix client for the webcam table
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:55'
updated_date: '2026-10-08 00:05'
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
- [x] #1 The webcam table connects through a library-backed realtime transport (socketioxide plus socket.io-client unless research rules it out); web/channels/protocol.rs, the Phoenix-style pubsub fastlane, and phx_* events are gone, and the phoenix and @types/phoenix packages are removed
- [x] #2 Joining, leaving, seat signaling (offers, answers, ICE, restarts), game state, board cards, trackers, log, timer, and presence work for multiple seats; a crashed or ended room makes clients rejoin or close as before
- [x] #3 Connection auth uses the native socket token; invalid, expired, or revoked tokens and logout or password change disconnect the socket (Rust tests)
- [x] #4 Per-connection event and signal rate limits, join limits, and the inbound size cap still apply (tests)
- [ ] #5 Frontend tests replace fake-phoenix with a fake for the new client; a portal check with two browser seats shows video and table state syncing
- [x] #6 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Server: socketioxide (0.18, extensions feature) layer on the router at /socket.io/ (WebSocket transport only, 384 KiB message cap, 1024-packet outbound buffer). Connect middleware authenticates auth.token with channels::authenticate and registers a forwarding MessageHandler (on_fallback) before the CONNECT packet, so events reach one task per socket in arrival order (socketioxide spawns ordinary handlers per message).
2. Per-socket task: join starts the table channel (previous one is stopped and awaited first, since channels join/leave the socket's room), other events go to it, not joined -> {error}. Session revocation disconnects the socket.
3. Channel: same validation and room logic; replies acked as {ok: response} / {error: reason}; broadcasts via io.to(topic) rooms (room.rs commits, deck_selected); Stop::Error -> rejoin {reason}; Stop::Close silent (table_closed/seat_replaced/leave ack already tell the client).
4. Presence: registry that emits the full roster (presence event) to the topic's room under its lock; leaves published on a broadcast channel for reveal clean-up. pubsub.rs/protocol.rs and phx_* deleted; axum ws feature dropped; AppState holds io + layer and a WeakAppState for the ns handlers.
5. Frontend: socket.io-client with auth function; TableChannel adapter (push().receive ok/error/timeout, join on every connect, rejoin with backoff after rejoin/join refusals, buffered pushes) replaces phoenix Socket/Channel/Presence; token refresh + manual reconnect on refused/server-closed connections; vite proxy /socket.io.
6. Tests: Rust harness client speaks Socket.IO v5/Engine.IO v4; tests updated for rosters, rejoin, connect_error. Frontend fake-socket-io replaces fake-phoenix; new table-channel tests.
7. Docs (webcam-table.md, rust/README.md, AGENTS.md), precommit, portal check with two seats.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Library: socketioxide 0.18.7 (server, tower layer on the axum router) + socket.io-client 4.8 (browser). No hand-written wire protocol remains; the Rust test harness has a ~100-line Socket.IO v5 client because no maintained Rust client fits the tokio test setup.

Decisions:
- Ordering: socketioxide spawns a task per message handler, so one fallback MessageHandler forwards every event into an mpsc consumed by one task per socket; channel logic is unchanged and events stay ordered.
- Auth happens in the namespace connect middleware (refusals arrive as connect_error 'unauthorized'); the forwarder is registered there too, before the CONNECT packet, so the first event cannot be dropped.
- Replies: acks carry {ok: response} or {error: reason}. Join is the 'join' event with room_id in the payload (topic strings are gone from the wire); 'leave' ends the seat.
- Presence sends the full roster (one meta per peer) on every change instead of phx_ref diffs; leaves go to channels over a tokio broadcast so a reveal still ends when its target leaves.
- Failure: Stop::Error emits 'rejoin' {reason}; normal stops are silent because table_closed/seat_replaced/leave already tell the client.
- Frontend: TableChannel (table-channel.ts) keeps the push().receive(ok|error|timeout) / on() shape so the hooks barely changed; it joins on every connect, rejoins with backoff, and buffers pushes until joined. Refused or server-closed connections reconnect manually with a refreshed token (Socket.IO does not retry those).
- WebSocket transport only (no long-polling), 384 KiB message cap, 1024-packet outbound buffer.

Validation: mise run precommit passed (610 integration tests, 285 frontend tests incl. new table-channel tests, build). Portal check with two signed-in browser seats (Cody/dev admin and Mara/seat2): presence (2/10, both seats listed), life change 40->33 and a d20 roll appeared on the other seat, both SFU peer connections connected, and both seats rejoined on their own after a server restart. AC #5 is left unchecked: remote video frames did not arrive in this orb because the SFU only binds loopback here (the server logs say so). The pre-change build (3ba0771) behaves the same in the same orb, so this is not a regression, but cross-seat video still needs a check on a real host.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the Phoenix Channels V2 protocol with Socket.IO: socketioxide on the server and socket.io-client in the React app. Removed protocol.rs, the pubsub fastlane, phx_* events, the phoenix packages, and axum's ws feature. Auth, revocation, rate and join limits, and the size cap still apply and are tested. Presence now sends full rosters, and a failed channel tells the client to rejoin. Verified with mise run precommit and a two-seat portal check covering presence, life, the log, and reconnecting after a restart. Cross-seat video could not be shown in the orb; the old build has the same limit there.
<!-- SECTION:FINAL_SUMMARY:END -->
