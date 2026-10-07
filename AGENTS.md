# AGENTS.md

## Project Structure

The Gathering is a self-hosted Commander (Magic: The Gathering) game tracker: a Rust JSON API and realtime server plus a Vite/React single-page app, shipped as one container or as a tarball for the Proxmox LXC installer.

- `rust/` — the backend (Cargo workspace; see `rust/README.md`): `crates/the-gathering` (axum + sqlx server: API, webcam table channels over the Phoenix Channels wire protocol the frontend's `phoenix` client speaks, Discord bot, background jobs) and `crates/sfu` (the webcam table's str0m WebRTC SFU). Shared Magic code (Scryfall, deck-list sources, name normalization, commander rules) comes from the `lotus` git dependency; report lotus gaps instead of forking it here.
  - `rust/migrations/*.sql` — the schema's only source, one file per migration (`<14-digit UTC version>_<name>.sql`). The versions continue the numbering existing databases record in `schema_migrations`.
  - `rust/schema.sql` — the generated dump of those migrations, committed for review; `rust/.sqlx` — committed query metadata for offline builds.
  - `rust/crates/the-gathering/src/web/` — router (`mod.rs`), auth guards, session/CSRF, the SPA shell, and the `/api` handlers in `web/api/`.
  - `rust/crates/the-gathering/tests/integration/` — HTTP and domain tests, compiled as one test binary (`main.rs` lists the modules; add a new file there); `support/` is the harness. `tests/fixtures` holds recorded payloads.
- `assets/react/` — the React app. Product code is organized under `src/features/` (games, decks, imports, and admin); thin TanStack Router adapters live in `src/routes/`, and `routeTree.gen.ts` is generated. Shared presentation and UI primitives remain in `src/components/`. Tailwind 4 + daisyUI themes live in `src/app.css`.
- `priv/static/` — static files the server serves; the frontend build writes `priv/static/assets/react`.
- Card-recognition models (dataset, training, evaluation, ONNX bundle export and publishing) live in the separate [Oracle](https://github.com/cfbender/oracle) repository, which ManaVault shares; this app only serves published bundles from `DATA_DIR/cardid`.
- `Dockerfile`, `docker-entrypoint.sh`, `docker-compose.yml` — production container build and startup flow; `deploy/proxmox/` — the LXC installer/updater and its test; `.github/workflows/release.yml` — release tarballs (tags, `nightly` from main, `preview` from other branches).
- `mise.toml` — pinned toolchain (Rust, SQLite, Node, aube) and tasks.
- `.agents/setup` and `.agents/resume` — orb bootstrap scripts; `.amp/services.yaml` — the review portal service.

## Common Commands

Run commands through `mise` to use the pinned toolchain:

```sh
mise install
mise run setup                         # dependencies, server build, dev database with demo data
mise run dev                           # Rust server on $PORT (default 4000) + Vite dev server on 5173
mise run rust:check                    # schema dump check, cargo fmt --check, clippy -D warnings, cargo test (in rust/)
mise run precommit                     # rust:check + aube run precommit
mise run rust:new-migration add_thing  # creates rust/migrations/<UTC timestamp>_add_thing.sql
mise run rust:sqlx-prepare             # rebuild rust/schema.sql and rust/.sqlx after a schema or query change
```

Run cargo directly in `rust/` with `mise exec -- cargo ...`. While developing Rust code, check queries against the schema directly: `mise run rust:schema`, then in `rust/` export `SQLX_OFFLINE=false DATABASE_URL="sqlite://$PWD/target/schema.db?mode=ro"`.

The server binary also has maintenance commands: `the-gathering migrate`, `seed` (dev demo data), `create-admin USERNAME`, `bootstrap-admin`, `catalog-sync`, and `catalog-backfill`. `THE_GATHERING_ENV` selects `dev`, `test`, or `prod` defaults (prod when unset).

JavaScript tooling goes through aube (the package manager) and Vite Plus (`vp`):

```sh
mise exec -- aube install --frozen-lockfile
mise exec -- aube run build             # production bundle into priv/static/assets/react
mise exec -- aube exec vp check         # fmt + lint + typecheck
mise exec -- aube exec vp test run
```

Use `mise exec -- aube` instead of invoking `aube` or npm directly. Fresh orbs do not expose aube on `PATH`. Note that `vp check` is a Vite Plus built-in, so call it via `aube exec vp check` rather than the npm script.

In development the Vite dev server (port 5173, or `VITE_PORT`) is the browser entry point; it proxies everything except its own assets to the backend. In an orb, `.amp/services.yaml` already runs this stack (`mise run dev`) as the `the-gathering-review` service, so check `amp orb service status the-gathering-review` (or `ss -ltnp`) before starting another server, and reuse the existing one.

### Adding a migration

1. `mise run rust:new-migration <snake_case_name>` and write the SQL. Tables that must be rebuilt (SQLite cannot alter most constraints) start with `PRAGMA foreign_keys = OFF;`; the migrator then runs that file statement by statement instead of in one transaction.
2. Data changes that need code rather than SQL go in `data_step` in `rust/crates/the-gathering/src/db/migrate.rs`, keyed by the migration's version.
3. `mise run rust:sqlx-prepare` to regenerate `rust/schema.sql` and `rust/.sqlx`, and commit all three. The server applies pending migrations at boot (and with `the-gathering migrate`).

Never edit or renumber a migration that has shipped; existing databases have recorded its version.

Production/container commands are documented in `README.md`.

## Development Notes

- The backend is API-only: render UI in React and serve data from `/api` handlers. The server renders only the SPA shell.

### JSON API conventions

- Handlers live in `rust/crates/the-gathering/src/web/api/` and are routed in `web/mod.rs`, grouped by the guards they need (`require_authenticated_user`, `require_admin`, `require_sudo_mode`) applied with `route_layer`. Admin-sensitive changes use both `require_admin` and `require_sudo_mode`.
- Handlers return `Result<_, ApiError>` (`src/error.rs`) instead of building error responses by hand. Validation errors render as `{"errors": {"field": ["message"]}}`; other errors as `{"errors": {"detail": "..."}}`.
- Successful responses wrap the payload in `{"data": ...}` (single object or list) via `web::api::data`.
- Use plural resource paths and standard REST actions (`GET /api/games`, `POST /api/games`, `GET /api/games/:id`, `PATCH`, `DELETE`). Paginate lists with `page`/`per_page` query params when they can grow unbounded.
- Every `/api` request that changes state needs the CSRF token; the frontend `api()` helper in `assets/react/src/lib/api.ts` sends it and rejects with `ApiError` (carrying `errors`) on non-2xx responses. Use it for all requests.
- Server state in React goes through TanStack Query (`useQuery`/`useMutation`, `QueryClientProvider` in `main.tsx`; the `queryClient` is also in router context). Key queries by resource, for example `["games", id]`.
- Authenticated handlers read the user from the `CurrentUser` request extension set by the auth layer.
- Use the shared `reqwest` clients for HTTP requests (Scryfall, Discord, deck-list sites); lotus's `DecklistClient` enforces the SSRF allowlist for deck links.
- Follow existing module and React component patterns. Keep changes small and focused.
- Frontend styling uses Tailwind utilities and daisyUI component classes; theme tokens are defined in `assets/react/src/app.css`. Use `cn()` from `src/lib/cn.ts` to merge classes. Shared primitives (Button, Card, Dialog, DropdownMenu, Popover, Select, Tabs, Switch, ToggleGroup, ported from ManaVault on Radix) live in `src/components/ui/`; page scaffolding (`PageHeader`, `PageSection`, `EmptyPanel`) is in `src/components/app-shell.tsx`. The "liquid glass" look is keyed on `html[data-theme-style="glass"]` (default; users can pick Classic in Settings), so glass rules in `app.css` must stay scoped to that attribute. Color palettes (Claret default, plus Nord, Catppuccin, Tokyo Night, Gruvbox, Everforest, Kanagawa, Night Owl, Dracula, Rosé Pine, Solarized, Monochrome) are a third orthogonal axis: `data-palette` on `<html>`, with per-mode token overrides in `src/palettes.css`. Palette and surface style are saved per user (`users.palette`/`users.theme_style`, `PATCH /api/session/appearance`), and the server's SPA shell (`rust/crates/the-gathering/src/web/shell.rs`) renders them onto `<html>` for the first paint. `localStorage` (`the-gathering:palette`, `the-gathering:theme-style`) is only the signed-out fallback, and light/dark stays per device. Palette ids live in `PALETTES` (`src/lib/theme.tsx`), `PALETTES` (`rust/crates/the-gathering/src/accounts/user.rs`), and `palettes.css`, so keep all three in sync. Keep palette roles consistent: secondary is the green, accent the gold used for winners.
- Rust backend: keep the lotus conventions (no `unsafe`; no `unwrap`/`expect`/`panic!`/indexing/`as` outside tests; literal regexes via `crate::regex::compile`; `sqlx::query!` macros checked against the schema; `crate::db::begin` for write transactions). JSON shapes, status codes, and error messages are what the frontend depends on; change them together with the frontend and add tests alongside behavior.
- Comments that mention "Elixir" record behavior of the earlier Elixir server that the Rust server stays compatible with (session cookies, signed tokens, stored import identities) or bugs it fixed; there is no Elixir code left.
- Run the narrowest relevant tests before reporting completion, and `mise run precommit` when a change is complete.
- Each Rust integration test gets its own temporary SQLite database (`tests/integration/support`), so tests run in parallel in one process. Process-wide state (the tracing subscriber) is shared: capture logs with `support::capture_logs()`, which is per thread.
- Compile times: dev builds are incremental and link with mold when installed (`rust/scripts/link-gcc`; `.agents/setup` installs it). `mise run rust:sweep` drops stale incremental caches; `mise run rust:bench` measures the dev loop (numbers in `rust/notes/compile-times.md`).
- CI (`.github/workflows/quality.yml`) runs the `precommit` steps as parallel jobs: the Rust job (schema dump check, online query check, `rust:check`, `deploy/proxmox/test.sh`) and the frontend job (`vp check`, `vp test run`, build). Keep the workflow in sync with the `precommit` task.
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
