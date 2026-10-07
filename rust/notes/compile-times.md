# Compile times

Measured on the orb (8 cores, 15 GB RAM) with `mise run rust:bench`
(`rust/scripts/bench-build.sh`), each run in a fresh scratch target directory. "Edit" appends
one constant to `crates/the-gathering/src/games/color_identity.rs`. Times are wall-clock seconds.

| Step                                  | Before | After  |
| ------------------------------------- | -----: | -----: |
| Cold `cargo build` (server binary)    |  163.2 |  160.3 |
| One-line edit, `cargo build`          |    5.5 |    5.5 |
| Cold `cargo check`                    |   76.1 |   31.3 |
| One-line edit, `cargo check`          |    2.7 |    2.6 |
| `cargo test --no-run` after the build |   77.5 |   65.7 |
| One-line edit, `cargo test --no-run`  |   40.2 |    7.7 |
| Target directory after the run        |  15 GB | 4.6 GB |

- Before: commit 5b4e334, 43 integration-test binaries, GNU ld, dependencies with line-table
  debug info.
- After: one integration-test binary, mold, no debug info for dependencies.
- The same edit-then-test step with GNU ld instead of mold, after the change, takes 11.6 s.
- For comparison, a non-incremental rebuild of the server crate (`CARGO_INCREMENTAL=0`, the
  setting ManaVault had) takes 29.2 s, against 5.5 s incremental.

## What changed

- **Incremental compilation** was already on for dev builds. Unlike ManaVault, this workspace
  never set `incremental = false`. CI (`quality.yml`), the release workflow, and the Docker build
  now set `CARGO_INCREMENTAL=0`, because they build from scratch and would only grow their
  caches. The orb setup's prebuild stays incremental, since it is the cache the first edit
  reuses. `mise run rust:sweep` drops incremental caches untouched for a week; run `cargo clean`
  when the target is still too large.
- **Linker:** `.cargo/config.toml` links Linux GNU targets through `scripts/link-gcc`, which
  uses mold when it is installed, else lld, else `cc`. `.agents/setup` installs mold. Docker
  (musl target) and machines without mold use the system linker unchanged.
  `THE_GATHERING_LINKER=default` forces the system linker.
- **Dev profile:** the workspace keeps `debug = "line-tables-only"`. Dependencies get
  `debug = 0` (they stay at `opt-level = 2`).
- **One test binary:** the integration tests moved from `tests/*.rs` (43 binaries, each linking
  the whole server) into modules of `tests/integration/main.rs`. This is most of the
  edit-then-test win and the target-size drop. The tests now run in parallel in one process.
  The one test that installed its own global log subscriber now uses the shared per-thread
  `support::capture_logs()`.

## Why the server is still one crate

`crates/the-gathering/src` is 38,314 lines in 11 modules. The largest are discord 6.5k, web
6.3k, games 5.5k, imports 5.3k, webcam 2.9k, and catalog 2.9k. That is large enough that a split
was worth measuring, but the numbers say it would not pay for itself:

- With incremental compilation, an edit already rebuilds in 5.5 s and checks in 2.6 s. That
  is under ManaVault's incremental 9.7 s and 4.2 s.
- Compiling the whole crate from scratch takes 25.8 s (`cargo build --timings`, lib target, not
  incremental) of the 160 s cold build. Dependencies are the rest.
  - Splitting could at best overlap part of those 25.8 s on a cold build.
  - An edit in a shared crate (core, accounts, catalog, games) would rebuild every crate above
    it. That can be slower than the incremental single crate.
- The module graph has cycles that a split would first have to untangle: accounts↔games,
  games↔catalog↔decklists, db→games (color identity), and webcam→web (pubsub). Most domain
  modules also take the whole `AppState`.

The test binaries and the linker were the real cost, and both are fixed above. Revisit the
split if an edit-then-build climbs past roughly 15 s. The obvious first cut is a core crate
(config, db, error, changeset, crypto, regex) under the domain crates, with jobs that need
`AppState` moved into the server crate.
