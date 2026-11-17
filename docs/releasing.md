# Releasing The Gathering

`git-cliff` writes `CHANGELOG.md` from the Conventional Commits since the previous `vX.Y.Z`
tag, and `scripts/bump.sh` bumps the version, tags, and pushes. `mise install` installs
`git-cliff`.

## Changelog

Preview or regenerate the changelog for the next version:

```sh
mise run changelog -- patch   # or minor, major
```

The script reads the version from `rust/crates/the-gathering/Cargo.toml` and computes the next
one. When that version is already ahead of the latest tag, it is used as is. It writes the
`vX.Y.Z` section at the top of `CHANGELOG.md`, replacing that section from an earlier run.
Edit the section before releasing if needed.

## Cut a release

Commit the changes to release, then run from `main`:

```sh
mise run release -- patch   # or minor, major
```

The working tree must be clean apart from `CHANGELOG.md`. The task:

- regenerates `CHANGELOG.md`
- sets the version in `rust/crates/the-gathering/Cargo.toml`, `rust/Cargo.lock`, and
  `package.json`
- updates the pinned Docker tag examples in `docs/installation.md`
- commits `chore: release vX.Y.Z` and creates an annotated `vX.Y.Z` tag
- pushes the branch and the tag to `origin`

The tag starts two workflows. `.github/workflows/release.yml` builds the release tarball and
publishes the GitHub release that the Proxmox LXC installer and the admin update check read.
`.github/workflows/container.yml` publishes the `X.Y.Z` and `X.Y` images to GitHub Container
Registry.

To print the planned version and changelog range without writing, committing, tagging, or
pushing:

```sh
THE_GATHERING_RELEASE_DRY_RUN=1 mise run release -- patch
```
