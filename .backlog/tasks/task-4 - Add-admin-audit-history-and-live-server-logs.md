---
id: TASK-4
title: Add admin audit history and live server logs
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-10-10 05:30'
updated_date: '2026-10-10 07:06'
labels: []
dependencies: []
references:
  - >-
    https://github.com/cfbender/manavault/blob/main/rust/crates/manavault-core/src/logs.rs
type: feature
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Administrators need to investigate user operations and inspect current server activity without shell access. The requested live view should follow ManaVault, whose bounded transient tracing stream is separate from persistent audit history. Research found no existing audit or log-view feature in The Gathering.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Administrators can browse persisted user-operation records with actor, operation, target, timestamp, and outcome; audit coverage is confirmed before implementation.
- [x] #2 Administrators can view live server logs with timestamps, severity, connection state, and a bounded browser buffer, similar to ManaVault.
- [x] #3 Both views enforce server-side administrator authorization, and audit records exclude credentials, tokens, and raw request bodies.
- [x] #4 Authorization and audit/log behavior are tested; affected UI states are exercised through the review portal; project precommit checks pass.
- [x] #5 All users are covered; safe before-and-after row snapshots are committed atomically with supported data changes, including deletes and related rows, with reconstruction limits documented.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Capture HTTP mutation attempts with actor snapshots, safe route/target, status and request ID; identify successful sign-in actors. Exclude page views and media/signaling traffic.
2. Capture explicit safe row snapshots with SQLite triggers in the same write transaction; associate through a transaction-local context set by db::begin and cleared before commit. Scope includes users (no credentials), players, decks, games/seats, settings, API-key metadata and import/Discord records. Rollback must discard snapshots, and concurrent requests must not share attribution.
3. Add admin/sudo paginated operation history and change detail APIs and UI. Record meaningful realtime table actions separately where practical.
4. Add bounded live tracing logs and a secured live endpoint with ongoing session checks; preserve stdout logging.
5. Test persistence, rollback/concurrency, secret exclusion and permissions; verify affected desktop/mobile/error states in the review portal and run precommit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented all-user mutation history across HTTP, Discord handlers, and meaningful webcam controls. Safe trigger snapshots cover supported relational data; secrets, transient state, scheduled queues and external effects are deliberately excluded, with reconstruction limits documented in README. Real-router tests prove member create/update/rejected update/delete reconstruction, actor retention, cascade snapshots, permissions, attribution and secret exclusion. Storage tests cover transaction rollback and concurrent attribution. Live SSE tests cover bounded buffers, filtering and revoked access. Portal verified desktop and narrow layouts, before/after, failed operations, live delivery, Clear, and logout disconnection. mise run precommit passed: 741 Rust tests and 502 frontend tests plus schema, fmt, clippy, frontend lint/typecheck and production build. Final frontend check and 21 admin tests passed after pagination wording adjustment. Review service remains running. Work is local, uncommitted and not pushed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added persisted administrator audit history for operations by all users, transactional safe before/after snapshots and a bounded live server-log view modeled on ManaVault. Verified with the full precommit suite, real-router reconstruction/security tests and browser checks through the review portal. Documented coverage, indefinite retention, secret exclusions and manual reconstruction limitations. Not committed, pushed or deployed.
<!-- SECTION:FINAL_SUMMARY:END -->
