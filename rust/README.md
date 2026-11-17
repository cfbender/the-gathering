# Rust backend

The Gathering's server. It serves the JSON API, realtime webcam tables, the Discord bot, and
background jobs on one SQLite database, and serves the React frontend.

Shared Magic code (Scryfall models and bulk data, deck-list sources, name normalization,
commander rules) comes from [lotus](https://github.com/cfbender/lotus); app-specific code
lives here. Setup, the dev server, and project-wide conventions are in the root
[README](../README.md) and [AGENTS.md](../AGENTS.md).

## Architecture

```text
browser ──▶ socketioxide layer ─── /socket.io/ ──▶ table socket task ──▶ table channel
   │            │                                                  │
   │            └─▶ static files (priv/static)                     ├─▶ room task (webcam/)
   │                                                               └─▶ SFU room (crates/sfu)
   └──▶ request id + trace ──▶ session ──▶ CSRF ──▶ current user ──▶ audit ──▶ guards ──▶ handler
```

**Requests.** `web::router` builds one axum router. The Socket.IO layer and static files sit
outside the logged stack. Every other request gets a request id and a tower-http trace line
(method, path, status, latency; never bodies or query strings), then passes these middlewares:

- `session_layer`: decrypts the session cookie into a typed `SessionData` (an axum-extra
  private cookie) and writes it back only when a handler changed it.
- `csrf_layer`: rejects state-changing requests whose `x-csrf-token` does not match the
  session's token.
- `current_user_layer`: looks up the session's `users_tokens` token and puts the user in the
  request extensions.
- `audit::layer` (`web/audit.rs`): records each state-changing request as an audit operation;
  database triggers copy the rows it changed into `audit_changes` (`audit.rs`).

Route groups add guards with `route_layer`: `require_authenticated_user`, `require_admin`,
`require_sudo_mode`. `/api/v1` is separate: it authenticates personal API keys and has its own
rate limit.

**Handlers.** Handlers in `web/api/` take typed inputs through `web/extract.rs`:
`JsonBody<T>`, `QueryParams<T>`, and `PathParam<T>` deserialize with serde, and a wrong type or
a missing field is a 400. `Patch<T>` (`patch.rs`) tells an absent field from `null` in partial
updates. Domain checks build a `ValidationError` (`validation.rs`), rendered as a 422 with
`{"errors": {"field": ["message"]}}`. Every other failure is an `ApiError` (`error.rs`), and
successes are wrapped as `{"data": ...}`. Players, decks, games, and stats are scoped to the
user's active playgroup (`user.playgroup()`).

**Sessions and auth.** The cookie carries only a token, and the `users_tokens` row behind it
grants access, so signing out or revoking a session ends it everywhere. Passwords are bcrypt.
Stored credentials (ManaVault API keys) and socket tokens are sealed with XChaCha20-Poly1305
(`crypto.rs`) under keys derived from `THE_GATHERING_SECRET_KEY`.

**Realtime tables.** The webcam table speaks Socket.IO (socketioxide on the server,
`socket.io-client` in the browser). `web/channels/` authenticates a socket while it connects,
with a sealed token from `GET /api/webcam-table/config`. Each socket's events go to one task,
in order, and a `join` starts a table channel (`webcam_table.rs`). The channel validates events
and rate-limits them per connection. It forwards game changes to the room task
(`webcam/room.rs`: one tokio task per table, which saves before it broadcasts) and media
signaling to the SFU (`crates/sfu`, str0m). Broadcasts go to a Socket.IO room per table, and
`presence.rs` sends each table its roster whenever it changes.

**Background tasks.** `serve` starts these, each a tokio task holding `AppState` (there is no
job queue):

- the scheduled Scryfall catalog sync (`catalog/sync_server.rs`)
- the Comprehensive Rules check, when enabled (`rules/sync.rs`)
- the card-recognition bundle check (`card_id/update.rs`)
- the deck-list cache sweeper
- the webcam pruner, which closes idle rooms and deletes expired sessions
- the optional Discord bot and its `/newgame` scheduler

## Compatibility constraints

These keep existing installs working across upgrades and must not change casually:

- **Migrations:** versions continue the numbering existing databases record in
  `schema_migrations`. Never edit or renumber a migration that has shipped.
- **Session cookies and credentials from releases up to 0.2:** `legacy.rs` reads them, so
  upgrades keep people signed in and keep stored keys. It never writes them.
- **Passwords:** bcrypt hashes at the configured cost. Existing hashes keep working.
- **Stored identities:** imported games keep their `external_id` hashes (`imports/etf.rs`, a
  frozen encoding), so re-imports recognize them. Saved table sessions keep their
  `{"version": 2, ...}` JSON.
- **Stored timestamps:** text columns. `UtcDateTime` reads every stored form (`Z`, offsets,
  fractions, SQLite's `CURRENT_TIMESTAMP`) and writes `2026-10-06T21:21:40Z`.

## Layout

- `crates/the-gathering/`: the server (library plus the `the-gathering` binary).
  - `config.rs`: environment variables and rate limits.
  - `db/`: pool, migrator, `UtcDateTime`/`IsoDate` column types.
  - `crypto.rs`: random tokens, constant-time comparison, and sealed values.
  - `legacy.rs`: see "Compatibility constraints".
  - `validation.rs`, `error.rs`, `patch.rs`: see "Handlers".
  - `accounts/`, `playgroups.rs`, `games/`, `stats/`: users and sign-in, playgroups, players,
    decks, games, the deck chooser, and statistics. [`docs/data-model.md`](../docs/data-model.md)
    describes the game tables.
  - `catalog/`: the Scryfall sync (`the-gathering catalog-sync`, plus the scheduled run), the
    card-link backfill (`the-gathering catalog-backfill`), and the card image disk cache.
  - `card_id/`: the card-recognition bundle and corrections. `rules/` and `ai/`: the
    Comprehensive Rules and AI rulings.
  - `imports/`: CSV, Mythic Track, and Google Sheet imports and the portable export/import.
  - `audit.rs`, `logs.rs`: audit history and the live server-log stream.
  - `self_update.rs`: admin-triggered updates (a systemd request file or Watchtower's HTTP API)
    and the newest-build check against GitHub.
  - `seed.rs`: development demo data (`the-gathering seed`, dev only).
  - `web/`: router, session cookie (`session.rs`), CSRF, auth guards, extractors, request ids
    and logging, SPA shell, static files, and the `web/api/` handlers.
  - `discord/`: the optional Discord bot, started by `serve` when `DISCORD_BOT_TOKEN` is set:
    `twilight-gateway` for events, a small `reqwest` REST client behind the `DiscordApi` trait
    (tests use a recording fake), SpellBot tracking, `/log`, `/summary`, `/newgame` queues and
    their scheduler, and the legacy `/won` form.
  - `webcam/`: webcam table rooms (one tokio task per room), turns, timer, votes, log, cards,
    and saved sessions. `web/channels/`: the table socket on `/socket.io/`, presence rosters,
    and the table channel.
  - `tests/integration/`: HTTP and domain tests, compiled as one binary (`main.rs` lists the
    modules; `support/` is the harness). `tests/fixtures` holds recorded deck-list, Discord,
    and Scryfall payloads.
- `crates/sfu/`: the webcam table's WebRTC SFU (see its README).
- `migrations/`: the schema, one SQL file per migration.
- `schema.sql`: the schema those migrations produce (generated, committed for review).
- `.sqlx/`: committed query metadata for offline builds.
- `models/`: bounded TLA+/TLC models of webcam rematch isolation, with
  [reproduction instructions](models/README.md).

## Developing

Run cargo through mise in `rust/` (`mise exec -- cargo test`), or use the tasks from the
repository root (`mise run rust:check`, `mise run test`). Normal builds read `.sqlx/`
(`SQLX_OFFLINE=true` in `.cargo/config.toml`). While changing queries, check them against the
schema directly:

```sh
mise run rust:schema
cd rust
export SQLX_OFFLINE=false DATABASE_URL="sqlite://$PWD/target/schema.db?mode=ro"
mise exec -- cargo test
```

## Schema and migrations

`migrations/*.sql` is the schema's only source; each file is `<14-digit UTC version>_<name>.sql`.
AGENTS.md lists the steps for adding one.

- `scripts/schema.sh` (`mise run rust:schema`) applies every migration to an empty
  `target/schema.db` and dumps it to `schema.sql`. `rust:check` and CI fail when the committed
  dump is stale.
- `mise run rust:sqlx-prepare` rebuilds `target/schema.db`, compiles every `sqlx::query!`
  against it, and records the result in `.sqlx/`.
- At boot (and with `the-gathering migrate`) the server applies every migration missing from
  `schema_migrations`, each in its own transaction, or statement by statement when it turns
  `PRAGMA foreign_keys` off to rebuild a table. Data changes that need code go in `data_step`
  in `db/migrate.rs`, keyed by the migration's version.

## Compile times

Dev builds are incremental and link through `scripts/link-gcc` (mold when installed, else lld,
else the system linker); dependencies build optimized without debug info. CI, the release
workflow, and the Docker build set `CARGO_INCREMENTAL=0`. The integration tests are one binary,
so an edit links the server into one test executable instead of one per test file.

```sh
mise run rust:bench    # cold, edit-then-build, edit-then-check, edit-then-test timings
mise run rust:sweep    # drop incremental caches untouched for a week; cargo clean if still large
```

`notes/compile-times.md` has the measurements and explains why the server is still one crate.

## Conventions

The lints enforce the design: `unsafe` is forbidden, and `unwrap`, `expect`, `panic!`,
indexing, and `as` casts are denied outside tests. Literal regexes go through
`regex::compile`. Queries use the `sqlx::query!` family so they are checked against the schema,
and timestamp columns decode as `UtcDateTime` (`AS "col: UtcDateTime"`). JSON shapes, status
codes, and error messages are what the frontend depends on; change them together with the
frontend and its tests. AGENTS.md has the rest.
