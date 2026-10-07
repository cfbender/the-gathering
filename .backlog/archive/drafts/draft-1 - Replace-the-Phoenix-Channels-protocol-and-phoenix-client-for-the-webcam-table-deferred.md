---
id: DRAFT-1
title: >-
  Replace the Phoenix Channels protocol and phoenix client for the webcam table
  (deferred)
status: Draft
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:49'
labels: []
dependencies: []
type: enhancement
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Deferred by the TASK-1 audit. The webcam table uses the Phoenix Channels V2 wire protocol (`web/channels`: frames `[join_ref, ref, topic, event, payload]`, heartbeat, phx_join/phx_leave/phx_reply/phx_close/phx_error, Presence diffs with phx_ref metas) because the frontend uses the `phoenix` npm client (`features/webcam-table/use-room-channel.ts` and its consumers, plus `test-support/fake-phoenix.ts`). The protocol works and its maintained client provides reconnect with backoff, rejoin, push replies with timeouts, and presence sync. Replacing it means a hand-written TS client and server protocol, plus re-verifying multi-seat WebRTC signaling (offers, answers, ICE restarts, crop updates) in browsers. Only pick this up if the client library becomes a liability (unmaintained, bundle size, a feature the protocol cannot carry) or a typed protocol is wanted for other clients.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A written proposal compares the replacement protocol against keeping Phoenix Channels V2 before any code changes
<!-- AC:END -->
