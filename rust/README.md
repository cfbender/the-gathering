# Rust backend

The Gathering's server. It serves the JSON API, realtime webcam tables (over the Phoenix
Channels wire protocol the frontend's `phoenix` client speaks), the Discord bot, and
background jobs on a SQLite database, and serves the React frontend.

Shared Magic code (Scryfall models and bulk data, decklist sources, name normalization,
commander rules) comes from [lotus](https://github.com/cfbender/lotus); app-specific code
lives here.

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
    and saved sessions; `web/channels/`: the channels server (V2 JSON over
    `/socket/websocket`), pubsub, presence, and the `webcam_table:*` channel.
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
- Comments that mention "Elixir" record behavior of the earlier Elixir server that this one
  keeps compatible with (cookies, stored identities) or bugs it fixed.
