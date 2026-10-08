# Rust backend

The Gathering's server. It serves the JSON API, realtime webcam tables (Socket.IO through
socketioxide, which the frontend's `socket.io-client` connects to), the Discord bot, and
background jobs on a SQLite database, and serves the React frontend.

Shared Magic code (Scryfall models and bulk data, decklist sources, name normalization,
commander rules) comes from [lotus](https://github.com/cfbender/lotus); app-specific code
lives here.

## Architecture

```text
browser ──▶ socketioxide layer ─── /socket.io/ ──▶ table socket task ──▶ table channel
   │            │                                                  │
   │            └─▶ static files (priv/static)                     ├─▶ room task (webcam/)
   │                                                               └─▶ SFU room (crates/sfu)
   └──▶ request id + tower-http trace ──▶ session ──▶ CSRF ──▶ current user ──▶ guards ──▶ handler
```

**Requests.** `web::router` builds one axum router. The Socket.IO layer and static files
sit outside the logged stack. Every other request gets a request id and a tower-http trace
line (method, path, status, latency; never bodies or query strings), then passes three
middlewares:

- `session_layer`: decrypts the session cookie into a typed `SessionData` (axum-extra
  private cookie) and writes it back only when a handler changed it.
- `csrf_layer`: rejects state-changing requests whose `x-csrf-token` does not match the
  session's token.
- `current_user_layer`: looks up the session's `users_tokens` token and puts the user in
  the request extensions.

Route groups add guards with `route_layer`: `require_authenticated_user`, `require_admin`,
`require_sudo_mode`. `/api/v1` is separate: personal API keys, with its own rate limit.

**Handlers.** Handlers in `web/api/` take typed inputs through `web/extract.rs`. `JsonBody<T>`,
`QueryParams<T>` and `PathParam<T>` deserialize with serde; a wrong type or a missing field
is a 400. `Patch<T>` (`patch.rs`) tells "absent" from "null" in partial updates. Domain
checks build a `ValidationError` (`validation.rs`), rendered as a 422 with
`{"errors": {"field": ["message"]}}`. Every other failure is an `ApiError` (`error.rs`), and
successes are wrapped as `{"data": ...}`.

**Sessions and auth.** The cookie carries only a token, and the `users_tokens` row behind it
grants access, so logging out or revocation ends a session everywhere. Passwords are bcrypt.
Stored credentials (ManaVault API keys) and socket tokens are sealed with
XChaCha20-Poly1305 (`crypto.rs`) under keys derived from `THE_GATHERING_SECRET_KEY`.

**Realtime tables.** The webcam table speaks Socket.IO (socketioxide; the browser uses
`socket.io-client`). `web/channels/` authenticates a socket while it connects (a sealed token
from `GET /api/webcam-table/config`). Each socket's events go to one task, in order, and a
`join` starts a table channel (`webcam_table.rs`). The channel validates events and rate-limits
them per connection. It forwards game changes to the room task (`webcam/room.rs`: one tokio
task per table, which saves before it broadcasts) and media signaling to the SFU
(`crates/sfu`, str0m). Broadcasts go to a Socket.IO room per table, and `presence.rs` sends
each table its roster whenever it changes.

**Background tasks.** `serve` starts these:

- the scheduled Scryfall catalog sync (`catalog/sync_server.rs`)
- the decklist cache sweeper
- the webcam pruner, which closes idle rooms and deletes expired sessions
- the optional Discord bot and its `/newgame` scheduler

Each is a tokio task that holds `AppState`; there is no job queue.

## Compatibility constraints

These keep existing installs working across upgrades and must not change casually:

- **Migrations:** versions continue the numbering existing databases record in
  `schema_migrations`. Never edit or renumber a migration that has shipped.
- **Session cookies and credentials from releases up to 0.2:** `legacy.rs` reads them, so
  upgrades keep people signed in and keep stored keys. It never writes them.
- **Passwords:** bcrypt hashes at the configured cost. Existing hashes keep working.
- **Stored identities:** imported games keep their `external_id` hashes (`imports/etf.rs`,
  a frozen encoding), so re-imports recognize them. Saved table sessions keep their
  `{"version": 2, ...}` JSON.
- **Stored timestamps:** text columns. `UtcDateTime` reads every stored form (`Z`,
  offsets, fractions, SQLite's `CURRENT_TIMESTAMP`) and writes `2026-10-06T21:21:40Z`.

## Layout

- `crates/the-gathering/`: the server (library plus the `the-gathering` binary).
  - `config.rs`: environment variables.
  - `db/`: pool, migrator, `UtcDateTime`/`IsoDate` column types.
  - `crypto.rs`: random tokens, constant-time comparison, and sealed (XChaCha20-Poly1305)
    values for socket tokens and stored credentials.
  - `legacy.rs`: read-only decoders for the session cookie and encrypted credentials that
    releases up to 0.2 wrote, so upgrades keep sessions and stored keys.
  - `validation.rs`: `ValidationError` (field and row messages) and the `Validator` checks;
    `error.rs`: `ApiError` and the JSON error bodies; `patch.rs`: `Patch<T>` for fields of
    partial updates.
  - `accounts/`, `catalog/`, `decklists/`, `games/`, ...: domain modules. `catalog/` also
    holds the Scryfall sync (`the-gathering catalog-sync`, plus a scheduled run started at
    boot), the backfill (`the-gathering catalog-backfill`), and the card image disk cache;
    `card_id/` serves the card-recognition bundle and corrections; `imports/` parses and
    commits CSV, Mythic Track, and pasted Google Sheet history and the portable
    export/import.
  - `self_update.rs`: admin-triggered updates (a systemd request file or Watchtower's
    HTTP API) and the newest-build check against GitHub.
  - `seed.rs`: development demo data (`the-gathering seed`, dev only).
  - `web/`: router, the typed session in an encrypted cookie (`session.rs`), CSRF, auth
    guards, typed extractors with JSON rejections (`extract.rs`), request ids and tower-http
    request logging (method, path, status, latency; never bodies or query strings), SPA
    shell, static files, and `web/api/*` handlers.
  - `discord/`: the optional Discord bot (started by `serve` when `DISCORD_BOT_TOKEN` is
    set): `twilight-gateway` for events, a small `reqwest` REST client behind the
    `DiscordApi` trait (tests use a recording fake), SpellBot staging, `/log`, `/summary`,
    `/newgame` queues and their scheduler, and the legacy `/won` form.
  - `webcam/`: webcam table rooms (one tokio task per room), turns, timer, log, cards,
    and saved sessions; `web/channels/`: the table socket (Socket.IO on `/socket.io/`:
    connect auth, one ordered task per socket), presence rosters, and the table channel.
  - `tests/integration/`: HTTP and domain tests, compiled as one binary (`support/` is the
    harness); `tests/fixtures` holds recorded decklist, Discord, and Scryfall payloads.
- `crates/sfu/`: the webcam table's WebRTC SFU (see its README).
- `migrations/`: the schema, one SQL file per migration.
- `schema.sql`: the schema those migrations produce (generated, committed for review).
- `.sqlx/`: committed query metadata for offline builds.

## Commands

From the repository root:

```sh
mise install                          # Rust, SQLite, Node, aube
mise run setup                        # dependencies, build, dev database with demo data
mise run dev                          # Rust server + Vite dev server
mise run rust:check                   # schema check, cargo fmt --check, clippy -D warnings, cargo test
mise run rust:new-migration add_thing # creates rust/migrations/<timestamp>_add_thing.sql
mise run rust:sqlx-prepare            # after a migration or a query change
```

While developing, compile queries against the schema directly instead of `.sqlx/`:

```sh
mise run rust:schema
cd rust
export SQLX_OFFLINE=false DATABASE_URL="sqlite://$PWD/target/schema.db?mode=ro"
cargo test
```

## Compile times

Dev builds are incremental and link through `scripts/link-gcc` (mold when installed, else lld,
else the system linker); dependencies build optimized without debug info. CI, the release
workflow, and the Docker build set `CARGO_INCREMENTAL=0`. The integration tests are a single
binary, so an edit links the server into one test executable rather than forty.

```sh
mise run rust:bench    # cold, edit-then-build, edit-then-check, edit-then-test timings
mise run rust:sweep    # drop incremental caches untouched for a week; cargo clean if still large
```

`notes/compile-times.md` has the measurements and why the server is still one crate.

## Schema and migrations

`migrations/*.sql` are the schema's only source. Each file is
`<14-digit UTC version>_<name>.sql`; the versions continue the numbering existing databases
already record in `schema_migrations`, so they upgrade in place.

- `scripts/schema.sh` (`mise run rust:schema`) applies every migration to an empty
  `target/schema.db` and dumps it to `schema.sql`; `rust:check` and CI fail when the
  committed dump is stale.
- `rust:sqlx-prepare` rebuilds `target/schema.db`, compiles every `sqlx::query!` against it,
  and records the result in `.sqlx/`. Normal builds read `.sqlx/` (`SQLX_OFFLINE=true` in
  `.cargo/config.toml`).
- At boot (and with `the-gathering migrate`) the server applies every migration missing
  from `schema_migrations`, each in its own transaction (statement by statement when it
  turns `PRAGMA foreign_keys` off to rebuild a table). Data changes that need code rather
  than SQL go in `data_step` in `db/migrate.rs`, keyed by the migration's version.

## Conventions

- Lints are the design: `unsafe` is forbidden; `unwrap`, `expect`, `panic!`, indexing,
  and `as` casts are denied outside tests. Literal regexes go through `regex::compile`.
- Queries use the `sqlx::query!` family so they are checked against the schema. Timestamp
  columns decode as `UtcDateTime` (`AS "col: UtcDateTime"`).
- Error messages, JSON shapes, and status codes are what the frontend depends on; change
  them together with the frontend and its tests.
- Comments that name an earlier release or the Elixir server explain an on-disk format this
  server still reads (cookies, sealed credentials, stored identities, saved sessions); see
  "Compatibility constraints".
