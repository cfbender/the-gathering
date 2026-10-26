# Releasing The Gathering

Releases use the same helper as ManaVault: `git-cliff` generates `CHANGELOG.md` from the
Conventional Commits since the previous `vX.Y.Z` tag, and a script bumps the version, tags, and
pushes.

## Prerequisites

Release tooling is managed by `mise.toml`:

```sh
mise install
```

## Generate Or Update The Changelog

Preview or regenerate the changelog for the next version:

```sh
mise run changelog -- patch
mise run changelog -- minor
mise run changelog -- major
```

The script reads the current version from `rust/crates/the-gathering/Cargo.toml`, calculates the
next version, and writes a section for the `vX.Y.Z` tag at the top of `CHANGELOG.md` (replacing
that version's section from an earlier run). Edit the generated section before releasing if needed.

## Cut A Release

Run the release task from a clean `main` branch after the desired changes are committed:

```sh
mise run release -- patch
```

The task:

- generates `CHANGELOG.md`
- bumps the `rust/crates/the-gathering/Cargo.toml` version (and `rust/Cargo.lock`) and the
  `package.json` version
- updates the README Docker tag example
- commits `chore: release vX.Y.Z`
- creates an annotated `vX.Y.Z` tag
- pushes the current branch and the tag to `origin`

The tag push triggers `.github/workflows/release.yml`, which builds the release tarball and
publishes the GitHub release that the Proxmox LXC installer and the admin update check use, and
`.github/workflows/container.yml`, which publishes the `X.Y.Z` and `X.Y` container images to GitHub
Container Registry.

Dry run:

```sh
THE_GATHERING_RELEASE_DRY_RUN=1 mise run release -- patch
```
