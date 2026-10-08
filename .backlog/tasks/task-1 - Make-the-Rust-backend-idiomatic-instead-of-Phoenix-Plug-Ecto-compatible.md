---
id: TASK-1
title: Make the Rust backend idiomatic instead of Phoenix/Plug/Ecto-compatible
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:47'
updated_date: '2026-10-08 01:39'
labels: []
dependencies: []
priority: medium
type: enhancement
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Rust server (axum, sqlx, tokio, str0m) was ported 1:1 from the Elixir/Phoenix app and still emulates Phoenix/Plug/Ecto wire formats and conventions. The owner wants it shaped as if written in Rust from scratch with the same libraries (no ORM switch; sqlx stays). Hard constraints: bcrypt password hashes stay as they are (nobody re-hashes); existing databases keep working (new migrations only, never renumber shipped ones); existing signed-in sessions and stored secrets keep working; the React frontend keeps working and is updated in the same subtask as any API change. Lotus is a dependency: report gaps, do not fork. Mirrors cfbender/manavault TASK-94. Each subtask is independently shippable and must pass `mise run precommit`.

## Audit (2026-10-07, main at daba2b5)

### Must stay compatible (data on disk, stored secrets, live sessions, external clients)
- `users.hashed_password`: bcrypt (`accounts/user.rs`). Unchanged.
- `users_tokens`: DB-backed session tokens (phx.gen.auth design: 14-day validity, reissued after 7 days, deleted on logout or admin revoke). The table and its token semantics are a sound design and stay. Only the cookie that carries the raw token changes, so live sessions must survive by reading the old cookie once.
- `users.manavault_api_key`: encrypted with `Plug.Crypto.encrypt/4` (an `XCP.` XChaCha20-Poly1305 token around an Erlang `term_to_binary({data, signed_at, :infinity})`, key from PBKDF2(`SECRET_KEY_BASE`, salt `the_gathering.accounts.encrypted_string`)). Existing values must still decrypt. They can be re-encrypted into a native format.
- The `SECRET_KEY_BASE` value: it decrypts those keys. A rename must accept the old name.
- Import identities: `imports/etf.rs` hand-encodes Erlang `term_to_binary` so CSV, Mythic Track, and Google Sheet re-imports hash to the `external_id`/`sheet_import_receipts`/`sheet:<key>` values the Elixir importer stored. The input rows cannot be recovered, so this frozen hash encoding stays. Only its framing changes.
- Stored text formats: timestamps (`2026-10-06T21:21:40Z`, plus `CURRENT_TIMESTAMP` and microsecond forms from migrations and the Elixir server), `webcam_table_sessions` JSON v2 BLOBs, and `expires_at` in `:utc_datetime_usec` form. The decoders keep reading every stored form.
- `schema_migrations(version, inserted_at)` and the 14-digit migration versions, together with `data_step` keyed by version.
- Operator configuration: `SECRET_KEY_BASE`, `PHX_HOST`, `PHX_SCHEME`, and `PHX_URL_PORT` in docker `.env` files and the Proxmox `/etc/the-gathering.env`.
- `GET /api/v1/games` (personal API keys, external clients) and the ManaVault share-link client: shapes stay.

### Exists only for Phoenix/Plug/Ecto compatibility (can change freely: only this server and its SPA see it)
1. Sessions: `crypto.rs` reimplements Plug.Crypto (KeyGenerator PBKDF2, MessageVerifier `SFMyNTY.`, Erlang external term format via `eetf`, `non_executable_binary_to_term`). `web/session.rs` is a string-keyed `HashMap<String, eetf::Term>` in the `_the_gathering_key` cookie under signing salt `sQwWhYdP`. The session keys are `user_token`, `live_socket_id` (a LiveView leftover that is written but never read), `user_return_to`, `registration_invite_hash`, `_csrf_token`, and `discord_oauth`, an Ueberauth-shaped `%{session_params: %{state}, return_to, sudo_discord_id, ...}` term map built by hand in `web/api/accounts.rs`.
2. CSRF: Plug's masked tokens (`crypto::csrf`: XOR mask, 24/56-character tokens). The SPA treats the token as opaque (`meta[name=csrf-token]`, `x-csrf-token`, refreshed from the response header), so a plain token needs no frontend change.
3. Socket tokens: `web/channels` mints `Phoenix.Token`-style `Plug.Crypto.encrypt` tokens (ETF payload, salt `webcam table socket`, one day). The client already refetches the token when the socket errors.
4. Config names: `SECRET_KEY_BASE` and `PHX_HOST`/`PHX_SCHEME`/`PHX_URL_PORT` (the Proxmox installer already takes a single `PUBLIC_URL` and splits it into those three).
5. Validation errors: `error::Errors` orders a field's messages newest first like `Ecto.Changeset.traverse_errors/2`, and nested rows replace a list's own messages (cast_assoc). `changeset.rs` is an `Ecto.Changeset` stand-in: casting from `serde_json::Value` (blank strings become nil, numeric strings become integers, "1"/"0" become booleans) plus `validate_*` helpers with Ecto's messages. `imports/inspect.rs` renders Elixir `inspect/1` syntax (`%{name: ["can't be blank"]}`, `nil`) into user-facing import errors.
6. Request parsing: `web/params.rs` is Plug.Parsers: query and body merged into one `Value`, `a[b]=c` nesting, `_json` wrapping, urlencoded bodies, and Phoenix.Logger `filter_parameters` debug logging. About 47 handlers take `Params` instead of typed `Json`/`Query`/`Path` extractors. Request bodies use Phoenix resource wrappers (`{"player": {...}}`). Path and body ids are cast "like Ecto" (`cast_id`, `parse_id`). `include_archived` honors only a JSON `true`, which a GET query string can never carry.
7. Router quirks: a method mismatch answers 404 (`method_not_allowed_fallback(not_found)`), `PUT` aliases `PATCH` because Phoenix `resources` generates both, and `web/mod.rs` comments map route groups to `pipe_through` lists. `request_id.rs` reimplements Plug.RequestId and Phoenix.Logger lines ("Sent 200 in 5ms"); tower-http's `TraceLayer` is already a dependency.
8. Framing and leftovers: rust/README.md, module docs, and comments describe the code as Elixir parity ("as Phoenix does", "Elixir bug fixed"). `webcam/turns.rs` keeps a field "for parity". The sfu README says it is "a port of the Elixir SFU". `accounts/user.rs` mimics Ecto's `inspect` redaction text. `UtcDateTime` and `IsoDate` are documented as `:utc_datetime` and `:date`. A few timestamps are still `String`: `discord/scheduled.rs` `joined_at`, `self_update.rs` `requested_at`, and `webcam/session.rs` usec strings.

### Phoenix-origin but staying (judgment calls)
- Phoenix Channels V2 wire protocol plus Presence for the webcam table (`web/channels`, about 1.8k lines of Rust; the frontend uses the `phoenix` npm client in `features/webcam-table`). The protocol is sound and documented (refs, join/leave, heartbeat, presence diffs), and its maintained client provides reconnect, rejoin, push replies with timeouts, and presence sync. Replacing it means a hand-written TS client and re-verifying multi-seat WebRTC signaling, at high cost for little gain. Phase: keep the protocol, describe the server as an implementation of it, and track replacement as a separate deferred draft.
- `schema_migrations`: the name is generic (Ecto, Rails, golang-migrate). Moving to `sqlx::migrate!`/`_sqlx_migrations` would need a bootstrap import with checksums, `-- no-transaction` handling for `PRAGMA foreign_keys = OFF` rebuilds, and data steps between migrations. That adds risk to existing databases for no behavior gain. Keep the custom runner and drop only the Ecto framing.
- Response envelopes `{"data": ...}` and `{"errors": {"detail": ...}}` / `{"errors": {"field": [...]}}`: these are the documented API convention (AGENTS.md) that the SPA and API-key clients use. Keep the shapes. Message order and nesting semantics become ours.
- `users_tokens` session design (see above).

### Not present here (unlike ManaVault)
- No Oban/jobs table: background work runs as tokio tasks.
- No GraphQL/Absinthe.
- No parity test harness against the Elixir app (`rust/notes/compile-times.md` is about build times).
- No crate re-exports that keep old module paths: the server is one crate, and its `pub use` lines are in-crate conveniences.

### Lotus gaps
None found during the audit. Gaps found during implementation will be recorded in the subtask notes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Every subtask is Done
- [x] #2 rust/README.md and module docs describe the architecture without framing it as an Elixir port; remaining Elixir references only record on-disk compatibility (stored identities, legacy cookie/secret readers)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner decision 2026-10-07: replace the Phoenix Channels protocol within TASK-1 (TASK-1.8, socketioxide + socket.io-client preferred); DRAFT-1 archived. Other audit decisions approved as proposed.

Finalized 2026-10-08. All eight subtasks are Done and pushed to main.

Open item: TASK-1.8 AC #5 (cross-seat video in a portal check) is unchecked. This orb's SFU binds only loopback, so remote frames cannot flow between browsers here, and the pre-change build behaves the same. Presence, table state, signaling, and rejoin were verified with two browser seats.

Follow-ups found during the work, not started:
1. The SFU announces private LAN host candidates beside the public IP. Chrome 142+ Local Network Access can then prompt remote players ('Access other apps and services on this device', or the local-network wording). Denying it only drops those candidates.
2. A phone whose network flaps (offline/online every 10-20s, likely Tailscale) rebuilds its media connection on every reconnect. The 5e6c6fa fix removed duplicate seats and the churn. Keeping the media connection across a short drop would need a server-side seat resume.
3. DRAFT-2: remove the legacy cookie and XCP readers after the deprecation window.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Made the Rust backend idiomatic instead of Phoenix/Plug/Ecto-compatible, in eight shipped phases:
- typed private-cookie sessions and a plain CSRF token
- renamed configuration with a deprecation window
- ValidationError
- typed extractors and flat request bodies
- native 405s and request logging
- Socket.IO in place of Phoenix Channels for the webcam table
- typed timestamps
- architecture docs

Password hashes, existing sessions, sealed secrets, stored identities, saved table sessions, and migration numbering all stay compatible. Each phase passed mise run precommit and was pushed to main.
<!-- SECTION:FINAL_SUMMARY:END -->
