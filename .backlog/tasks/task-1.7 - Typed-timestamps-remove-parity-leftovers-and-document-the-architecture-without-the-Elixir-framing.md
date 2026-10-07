---
id: TASK-1.7
title: >-
  Typed timestamps, remove parity leftovers, and document the architecture
  without the Elixir framing
status: To Do
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:49'
updated_date: '2026-10-07 21:55'
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
- [ ] #1 No record or row struct carries a timestamp as String; stored-format parsing is tested for every form on disk
- [ ] #2 rg -i "elixir|phoenix|ecto|plug" in rust/ matches only on-disk compatibility notes and the Phoenix Channels protocol/client name
- [ ] #3 rust/README.md is an architecture overview, and AGENTS.md conventions match the new session, validation, and request handling
- [ ] #4 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Typed timestamps: `joined_at`, `requested_at`, and the webcam session `expires_at`/`updated_at` become `UtcDateTime`/`OffsetDateTime` with an explicit stored-format codec (the usec text form stays on disk, and tests parse the second and microsecond forms). Reword the `db/time.rs` docs in terms of our storage formats.
2. Remove dead parity code (`turns.rs` field, the Ecto-mimicking `inspect` redaction text in `accounts/user.rs`, and similar items found by `rg -i 'elixir|phoenix|ecto|plug'`). Rewrite comments to state the behavior and drop the Elixir comparison. Keep a comment only where it explains on-disk compatibility (`imports/etf.rs`, legacy cookie and secret readers, stored identities, `schema_migrations`).
3. web/channels: document it as this server's implementation of the Phoenix Channels V2 protocol that the `phoenix` client speaks, covering frames, heartbeat, join/leave, close/error semantics, and presence diffs. The docs describe the protocol and drop the parity notes.
4. Rewrite rust/README.md as an architecture overview (request pipeline, sessions and auth, validation, channels and SFU, background tasks, migrations and data steps, compatibility constraints). Update the sfu README. Keep `rust/notes/compile-times.md`, which is a build-performance record. Update the AGENTS.md notes about Elixir comments, the JSON conventions, and the session cookie.
5. Run `mise run precommit`. Do a quick portal smoke test (sign in, game list, table list).
<!-- SECTION:PLAN:END -->
