# AGENTS.md

## Project Structure

The Gathering is a self-hosted Commander (Magic: The Gathering) game tracker: a Rust JSON API and realtime server plus a Vite/React single-page app, shipped as one container or as a tarball for the Proxmox LXC installer.

- `rust/` — the backend Cargo workspace (see `rust/README.md`):
  - `crates/the-gathering` — the axum + sqlx server: API, webcam table channels (Socket.IO), Discord bot, background jobs.
  - `crates/sfu` — the webcam table's str0m WebRTC SFU.
  - Shared Magic code (Scryfall, deck-list sources, name normalization, commander rules) comes from the `lotus` git dependency. Report lotus gaps instead of forking it here.
  - `migrations/*.sql` — the schema's only source, one file per migration (`<14-digit UTC version>_<name>.sql`). `schema.sql` is the generated dump and `.sqlx` the offline query metadata; both are committed.
  - `crates/the-gathering/src/web/` — router (`mod.rs`), auth guards, session/CSRF, the SPA shell, and the `/api` handlers in `web/api/`.
  - `crates/the-gathering/tests/integration/` — HTTP and domain tests in one test binary (`main.rs` lists the modules; add new files there). `support/` is the harness and `tests/fixtures` holds recorded payloads.
- `assets/react/` — the React app. Product code lives in `src/features/<feature>/`, thin TanStack Router adapters in `src/routes/` (`routeTree.gen.ts` is generated), and shared presentation and UI primitives in `src/components/`.
- `priv/static/` — static files the server serves; the frontend build writes `priv/static/assets/react`.
- Card-recognition models live in the separate [Oracle](https://github.com/cfbender/oracle) repository, which publishes bundles as `scanner-bundle-*` GitHub releases of this repository. The server downloads them into `DATA_DIR/cardid` (`card_id/update.rs`) and serves them from there.
- `Dockerfile`, `docker-entrypoint.sh`, `docker-compose.yml` — the production container; `deploy/proxmox/` — the LXC installer/updater and its test; `.github/workflows/release.yml` — release tarballs (tags, `nightly` from main, `preview` from other branches).
- `mise.toml` — pinned toolchain (Rust, SQLite, Node, aube) and tasks.
- `.agents/setup` and `.agents/resume` — orb bootstrap scripts; `.amp/services.yaml` — the review portal service.

Documentation lives in `docs/`: `installation.md` (install, configuration, operations; kept complete enough for an agent to set up a server from it), `features.md`, `administration.md`, and topic references linked from `README.md`, which shows the product with screenshots from `docs/images/`.

## Common Commands

Run everything through `mise` so the pinned toolchain is used:

```sh
mise install
mise run setup                         # dependencies, server build, dev database with demo data
mise run dev                           # Rust server on $PORT (default 4000) + Vite dev server on 5173
mise run rust:check                    # schema dump check, cargo fmt --check, clippy -D warnings, cargo test
mise run precommit                     # rust:check + frontend check, tests, and build
mise run rust:new-migration add_thing  # creates rust/migrations/<UTC timestamp>_add_thing.sql
mise run rust:sqlx-prepare             # rebuild rust/schema.sql and rust/.sqlx after a schema or query change
mise run release -- major|minor|patch  # changelog, version bump, tag, push (docs/releasing.md)

mise exec -- aube install --frozen-lockfile
mise exec -- aube run build            # production bundle into priv/static/assets/react
mise exec -- aube exec vp check        # fmt + lint + typecheck
mise exec -- aube exec vp test run
```

- Run cargo in `rust/` with `mise exec -- cargo ...`. To check queries against the schema while developing, run `mise run rust:schema`, then in `rust/` export `SQLX_OFFLINE=false DATABASE_URL="sqlite://$PWD/target/schema.db?mode=ro"`.
- Fresh orbs do not put cargo, aube, or vp on `PATH`; always go through `mise exec`. `vp check` is a Vite Plus built-in, so call it with `aube exec vp check`, not the npm script.
- The server binary has maintenance commands: `migrate`, `seed` (dev demo data), `create-admin USERNAME`, `bootstrap-admin`, `catalog-sync`, and `catalog-backfill`. `THE_GATHERING_ENV` selects `dev`, `test`, or `prod` defaults (prod when unset).
- In development the Vite dev server (port 5173, or `VITE_PORT`) is the browser entry point and proxies everything else to the backend. In an orb, `.amp/services.yaml` already runs `mise run dev` as the `the-gathering-review` service; check `amp orb service status the-gathering-review` before starting another server.

### Adding a migration

1. `mise run rust:new-migration <snake_case_name>` and write the SQL. A migration that rebuilds tables (SQLite cannot alter most constraints) starts with `PRAGMA foreign_keys = OFF;`, and the migrator runs it statement by statement instead of in one transaction.
2. Data changes that need code go in `data_step` in `rust/crates/the-gathering/src/db/migrate.rs`, keyed by the migration's version.
3. `mise run rust:sqlx-prepare` regenerates `rust/schema.sql` and `rust/.sqlx`; commit all three. The server applies pending migrations at boot and with `the-gathering migrate`.

Never edit or renumber a migration that has shipped; existing databases have recorded its version.

## Development Notes

- Follow existing module and component patterns, and keep changes small and focused.
- The backend is API-only: render UI in React and serve data from `/api` handlers. The server renders only the SPA shell.

### Backend

- Keep the lotus conventions: no `unsafe`; no `unwrap`/`expect`/`panic!`/indexing/`as` outside tests; literal regexes via `crate::regex::compile`; `sqlx::query!` macros checked against the schema; `crate::db::begin` for write transactions.
- Handlers live in `src/web/api/` and are routed in `web/mod.rs`, grouped by guard (`require_authenticated_user`, `require_admin`, `require_sudo_mode`) with `route_layer`. Admin-sensitive changes use both `require_admin` and `require_sudo_mode`. Authenticated handlers read the user from the `CurrentUser` request extension.
- Handlers take typed inputs through `web::extract::{JsonBody, QueryParams, PathParam}` (rejections are JSON 400, 413, or 415) and return `Result<_, ApiError>` (`src/error.rs`). Successful responses wrap the payload in `{"data": ...}` via `web::api::data`.
- Validation failures are a `ValidationError` (`src/validation.rs`, built with `Validator`) rendered as `{"errors": {"field": ["message"]}}` in check order, with list rows as `{"seats": [{}, {"field": ["message"]}]}`. Other errors render as `{"errors": {"detail": "..."}}`.
- Use plural resource paths and standard REST actions, and paginate unbounded lists with `page`/`per_page`. Request bodies are flat objects. Partial updates use `patch::Patch<T>` fields with `#[serde(default)]` so a missing key, `null`, and a value differ; missing required fields are 422, wrong JSON types 400.
- JSON shapes, status codes, and error messages are what the frontend depends on: change them together with the frontend and add tests alongside behavior.
- Use the shared `reqwest` clients for outbound HTTP; lotus's `DecklistClient` enforces the SSRF allowlist for deck links.
- The browser session is a typed `SessionData` in an encrypted cookie (`web/session.rs`); add fields there rather than new cookies.
- Timestamps are `UtcDateTime` or `IsoDate` (`db/time.rs`) in rows, records, and JSON, never `String`; decode columns with `AS "col: UtcDateTime"`.
- Comments that name an earlier release or the Elixir server explain an on-disk format this server still reads (`legacy.rs`, `imports/etf.rs`, saved table sessions, `schema_migrations` numbering). Write other comments about the behavior itself, not about how earlier code did it.

### Frontend

- Every state-changing `/api` request needs the CSRF token: use the `api()` helper in `src/lib/api.ts`, which sends it and rejects with `ApiError` (carrying `errors`) on non-2xx responses.
- Server state goes through TanStack Query (`QueryClientProvider` in `main.tsx`; the `queryClient` is also in router context). Key queries by resource, for example `["games", id]`.
- Style with Tailwind utilities and daisyUI classes, merged with `cn()` (`src/lib/cn.ts`). Theme tokens are in `src/app.css`. Shared primitives (ported from ManaVault on Radix) are in `src/components/ui/`; page scaffolding (`PageHeader`, `PageSection`, `EmptyPanel`) is in `src/components/app-shell.tsx`.
- Appearance has three independent axes: light/dark (per device), surface style (`data-theme-style`, `glass` by default or `classic`), and color palette (`data-palette`, tokens in `src/palettes.css`). Glass rules in `app.css` must stay scoped to `html[data-theme-style="glass"]`. Style and palette are saved per user, and the SPA shell (`web/shell.rs`) renders them onto `<html>` for the first paint; `localStorage` is only the signed-out fallback. Palette ids are listed in `PALETTES` in `src/lib/theme.tsx`, `PALETTES` in `rust/crates/the-gathering/src/accounts/user.rs`, and `palettes.css`; keep all three in sync. Secondary is the green and accent the gold used for winners in every palette.

### Webcam table and deck scripts

- The table's realtime connection is Socket.IO (`web/channels/`, socketioxide; `features/webcam-table/table-channel.ts` on the client). Events that expect a reply are acknowledged with `{"ok": response}` or `{"error": reason}`. Change server and client together, with tests on both sides (`tests/integration/webcam_*`, `table-channel.test.ts`). Design notes are in `docs/webcam-table.md`.
- Deck chooser scripts are sandboxed Lua 5.4 (mlua, vendored): `games/deck_script.rs` runs them and `games/chooser_context.rs` builds the `deck` and `ctx` tables. `docs/deck-chooser-scripts.md` is the field reference the script editor renders (the Dockerfile copies it into the frontend build). `tests/integration/deck_chooser_scripts.rs` fails when a field is missing from it or a shipped example in `features/decks/deck-scripts/` errors, so add fields to `chooser_context.rs` and the guide together.

### Testing and CI

- Run the narrowest relevant tests while working, and `mise run precommit` when a change is complete.
- Each Rust integration test gets its own temporary SQLite database, so tests run in parallel in one process. The tracing subscriber is shared: capture logs with `support::capture_logs()`, which is per thread.
- Dev builds are incremental and link with mold when installed (`rust/scripts/link-gcc`). `mise run rust:sweep` drops stale incremental caches; `mise run rust:bench` measures the dev loop (`rust/notes/compile-times.md`).
- CI (`.github/workflows/quality.yml`) runs the `precommit` steps as parallel Rust and frontend jobs, plus the online query check and `deploy/proxmox/test.sh`. Keep it in sync with the `precommit` task.
- For UI changes, verify the rendered result through the review portal and leave the service running.
- Update documentation when project structure, setup, or runtime behavior changes.

## Git Commit Policy

- Every Git commit must use a Conventional Commits message.
- Commit as the current thread's user using their configured Git identity.
- Never add `Co-authored-by` trailers or credit Amp, an AI agent, or another co-author.
- Never push unless the user asks. When they do, verify the commit has no co-authorship trailers, then push the current branch and confirm it matches its upstream.

<!-- BACKLOG.MD GUIDELINES START -->
<!-- backlog.md-instructions-version: 1.53.0 -->

<CRITICAL_INSTRUCTION>

## Backlog.md Workflow

This project uses Backlog.md for task and project management.

Use Backlog only for sizeable implementation work that is worth documenting because it benefits from durable planning, decisions, progress tracking, or handoff notes. Do not run `backlog instructions overview` or any other Backlog command automatically at the start of a request. Skip Backlog for questions, explanations, operational actions, commits and pushes, quick fixes, and small mechanical, configuration, or documentation changes.

When work genuinely warrants Backlog, run `mise exec -- backlog instructions overview`, search for an existing task first, and then read only the relevant task instructions. The Backlog CLI is managed by mise and may not be directly available on `PATH`, especially during first-time orb setup.

Before task lifecycle actions, read the matching detailed guide:

- `mise exec -- backlog instructions task-creation` before creating or splitting tasks
- `mise exec -- backlog instructions task-execution` before planning, changing status or assignee, adding a plan or implementation notes, or implementing task work
- `mise exec -- backlog instructions task-finalization` before checking acceptance criteria, writing final summaries, or moving tasks to terminal statuses

Use `mise exec -- backlog <command> --help` before running unfamiliar commands. Help shows options, fields, and examples.

Do not edit Backlog task, draft, document, decision, or milestone markdown files directly. Use `mise exec -- backlog` so metadata, relationships, and history stay consistent.

</CRITICAL_INSTRUCTION>

<!-- BACKLOG.MD GUIDELINES END -->
