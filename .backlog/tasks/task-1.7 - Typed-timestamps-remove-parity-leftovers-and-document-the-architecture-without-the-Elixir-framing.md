---
id: TASK-1.7
title: >-
  Typed timestamps, remove parity leftovers, and document the architecture
  without the Elixir framing
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:49'
updated_date: '2026-10-08 01:38'
labels: []
dependencies:
  - TASK-1.2
  - TASK-1.6
  - TASK-1.8
parent_task_id: TASK-1
priority: medium
type: enhancement
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the wire-format subtasks, leftovers of the port remain. rust/README.md, crate and module docs, and many comments frame the code as Elixir parity ("as Phoenix does", "Elixir bug fixed", `Ecto.UUID.cast`). `UtcDateTime` and `IsoDate` are documented as `:utc_datetime` and `:date`, and a few timestamps are still `String` (`discord/scheduled.rs` `joined_at`, `self_update.rs` `requested_at`, `webcam/session.rs` usec strings). `webcam/turns.rs` keeps a field "for parity". The sfu README describes itself as "a port of the Elixir SFU". The web/channels docs present the Phoenix Channels protocol as emulation rather than as the protocol this server implements. `imports/etf.rs` must stay (it is the frozen hash encoding of stored import identities) but should say so. AGENTS.md says Elixir comments record compatibility; after this subtask, that should hold only for real on-disk compatibility.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 No record or row struct carries a timestamp as String; stored-format parsing is tested for every form on disk
- [x] #2 rg -i "elixir|phoenix|ecto|plug" in rust/ matches only on-disk compatibility notes and the Phoenix Channels protocol/client name
- [x] #3 rust/README.md is an architecture overview, and AGENTS.md conventions match the new session, validation, and request handling
- [x] #4 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Typed timestamps: `joined_at`, `requested_at`, and the webcam session `expires_at`/`updated_at` become `UtcDateTime`/`OffsetDateTime` with an explicit stored-format codec (the usec text form stays on disk, and tests parse the second and microsecond forms). Reword the `db/time.rs` docs in terms of our storage formats.
2. Remove dead parity code (`turns.rs` field, the Ecto-mimicking `inspect` redaction text in `accounts/user.rs`, and similar items found by `rg -i 'elixir|phoenix|ecto|plug'`). Rewrite comments to state the behavior and drop the Elixir comparison. Keep a comment only where it explains on-disk compatibility (`imports/etf.rs`, legacy cookie and secret readers, stored identities, `schema_migrations`).
3. web/channels: document it as this server's implementation of the Phoenix Channels V2 protocol that the `phoenix` client speaks, covering frames, heartbeat, join/leave, close/error semantics, and presence diffs. The docs describe the protocol and drop the parity notes.
4. Rewrite rust/README.md as an architecture overview (request pipeline, sessions and auth, validation, channels and SFU, background tasks, migrations and data steps, compatibility constraints). Update the sfu README. Keep `rust/notes/compile-times.md`, which is a build-performance record. Update the AGENTS.md notes about Elixir comments, the JSON conventions, and the session cookie.
5. Run `mise run precommit`. Do a quick portal smoke test (sign in, game list, table list).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Typed timestamps:
- Discord roster Entry.joined_at is UtcDateTime. Unreadable or empty stored values read as the epoch, so one bad entry cannot fail a roster.
- Self-update Status.requested_at is Option<OffsetDateTime>, serialized as RFC 3339.
- Webcam session expires_at decodes and binds as UtcDateTime. Rows written by releases up to 0.2 keep microsecond text and still load and prune; new rows use whole seconds, and the mixed text forms order correctly to within a second.
- remote_decks updated_at is Option<UtcDateTime>, so decks from different hosts sort by time instead of by their differently formatted strings.
- Remaining String date fields are request query inputs, deliberately lenient (date_from/date_to, V1Query).

Tests: roster entries in seconds, micros, offset, blank, and missing forms; a session saved with a microsecond expiry still loads and prunes; UtcDateTime parses the micro+offset form.

Comment sweep (delegated to a subagent, reviewed): about 110 files under rust/ reworded from Elixir/Phoenix/Ecto/Plug parity and name/arity prefixes to plain descriptions. Six tests were renamed; nothing else changed. The departed turn-seat flag stays (it is used and tested), with a reworded comment. The remaining matches are legacy.rs, imports/etf.rs (frozen hash encoding), and google_sheet.rs's key-order note for stored identities.

Docs: rust/README.md now has an Architecture section (request pipeline, handlers and validation, sessions and auth, realtime tables, background tasks) and a Compatibility constraints section. The sfu README no longer frames itself as a port. AGENTS.md has session, timestamp, and table-socket conventions, and the comment rule now covers on-disk formats only.

Validation: mise run precommit passed (57 + 611 + 41 + 2 + 4 Rust tests, 489 frontend tests, build). Portal smoke test passed: games list, software-update status, open rooms list, and a table seat.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Typed every stored or record timestamp: Discord roster joined_at, self-update requested_at, webcam session expires_at, and remote deck updated_at. Added tests for each stored form, including microsecond rows from earlier releases. Removed the Elixir/Phoenix/Ecto/Plug parity framing from comments across rust/, keeping only on-disk compatibility notes. Rewrote rust/README.md as an architecture overview with a compatibility section, the sfu README, and the AGENTS.md conventions. Verified with mise run precommit and a portal smoke test.
<!-- SECTION:FINAL_SUMMARY:END -->
