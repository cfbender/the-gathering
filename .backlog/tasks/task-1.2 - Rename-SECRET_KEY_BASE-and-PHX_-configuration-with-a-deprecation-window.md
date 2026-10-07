---
id: TASK-1.2
title: Rename SECRET_KEY_BASE and PHX_* configuration with a deprecation window
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:48'
updated_date: '2026-10-07 22:19'
labels: []
dependencies:
  - TASK-1.1
parent_task_id: TASK-1
priority: medium
type: enhancement
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`config.rs` reads `SECRET_KEY_BASE` (Phoenix endpoint secret) and `PHX_HOST`, `PHX_SCHEME`, and `PHX_URL_PORT` (Phoenix endpoint URL), and the field is named `secret_key_base`. Operators have these names in docker `.env` files and in the Proxmox `/etc/the-gathering.env`, and the secret value decrypts stored ManaVault keys. The old names must keep working during a deprecation window. The Proxmox installer already asks for one `PUBLIC_URL` and splits it into the three PHX variables.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 THE_GATHERING_SECRET_KEY and THE_GATHERING_PUBLIC_URL are the documented names
- [x] #2 SECRET_KEY_BASE and PHX_HOST/PHX_SCHEME/PHX_URL_PORT still work and log a deprecation warning naming the replacement (unit tests)
- [x] #3 README, .env.example, docker-compose.yml, docs, and the Proxmox installer use the new names; deploy/proxmox/test.sh passes
- [x] #4 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. config.rs: rename the field secret_key_base to secret_key (read from THE_GATHERING_SECRET_KEY, falling back to SECRET_KEY_BASE with a warning) and replace url_scheme/url_host/url_port with public_url (an origin parsed from THE_GATHERING_PUBLIC_URL; http(s), a host, no path or query, default ports omitted). When the new name is unset, PHX_SCHEME/PHX_HOST/PHX_URL_PORT apply with the old defaults and a warning that names the equivalent new value. Leftover PHX_* alongside the new name are ignored with a warning.
2. Config::read(&dyn Fn(&str) -> Option<String>) -> (Config, warnings), built on a Vars reader; from_env logs the warnings. Unit tests cover the new names, the fallbacks, precedence, the defaults, and the errors.
3. .env.example and README use the new names. docker-compose passes the new names and still forwards the old ones (empty by default) so existing .env files keep working. docs/discord-integration.md is updated. The Proxmox installer computes PUBLIC_ORIGIN and patches either the new keys or, for an older VERSION's .env.example, the PHX_* keys. The updater does not rewrite existing env files, so rolling back to 0.2 keeps working; the server warning tells operators to rename.
4. Ran deploy/proxmox/test.sh with new parse_public_url cases and mise run precommit, and restarted the review service.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions: THE_GATHERING_PUBLIC_URL replaces three PHX_* parts with one origin (the Proxmox installer already took a single PUBLIC_URL). Old names keep working: SECRET_KEY_BASE logs "SECRET_KEY_BASE is deprecated; rename it to THE_GATHERING_SECRET_KEY", and PHX_* log the exact THE_GATHERING_PUBLIC_URL value to use. Existing Proxmox env files are deliberately not rewritten, so a rollback to 0.2 still starts. docker-compose forwards both old and new names, with no required-variable interpolation, so upgrading the compose file never breaks an old .env; the server enforces the secret instead. Lotus gaps: none.

Validation: mise run precommit exit 0 (55 unit tests including 5 new config tests, 606 integration tests, frontend checks and build). deploy/proxmox/test.sh: all checks passed, including 4 new parse_public_url cases. Ran the-gathering migrate with SECRET_KEY_BASE and PHX_HOST/PHX_SCHEME/PHX_URL_PORT and saw both deprecation warnings; THE_GATHERING_PUBLIC_URL=games.example.com fails with a URL error. The review service restarted cleanly (listening, /api/health ok, the games page renders in the portal stack).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Configuration now reads THE_GATHERING_SECRET_KEY and THE_GATHERING_PUBLIC_URL (one origin). SECRET_KEY_BASE and PHX_* still work and log deprecation warnings. Config::read makes this testable; .env.example, docker-compose, README, the Discord docs, and the Proxmox installer use the new names while staying compatible with older releases. Verified with mise run precommit, the new config unit tests, deploy/proxmox/test.sh, a CLI run showing the warnings, and a review-service restart.
<!-- SECTION:FINAL_SUMMARY:END -->
