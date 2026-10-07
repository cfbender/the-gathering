---
id: TASK-1.2
title: Rename SECRET_KEY_BASE and PHX_* configuration with a deprecation window
status: To Do
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:48'
updated_date: '2026-10-07 21:48'
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
- [ ] #1 THE_GATHERING_SECRET_KEY and THE_GATHERING_PUBLIC_URL are the documented names
- [ ] #2 SECRET_KEY_BASE and PHX_HOST/PHX_SCHEME/PHX_URL_PORT still work and log a deprecation warning naming the replacement (unit tests)
- [ ] #3 README, .env.example, docker-compose.yml, docs, and the Proxmox installer use the new names; deploy/proxmox/test.sh passes
- [ ] #4 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Rename the config field `secret_key_base` to `secret_key`, read from `THE_GATHERING_SECRET_KEY` (falling back to `SECRET_KEY_BASE` with a startup warning; same 64-byte minimum). Replace `url_scheme`/`url_host`/`url_port` with one parsed `public_url: url::Url` read from `THE_GATHERING_PUBLIC_URL` (for example `https://games.example.com`). When it is unset, fall back to `PHX_SCHEME`/`PHX_HOST`/`PHX_URL_PORT` with the same defaults and a warning.
2. Split `Config::from_env` into a reader over a lookup (`Config::read(&dyn Fn(&str) -> Option<String>) -> (Config, Vec<Warning>)`) so unit tests cover the new names, the fallbacks, the warnings, and the errors. `from_env` logs the warnings.
3. Update `.env.example`, `docker-compose.yml`, README (configuration table and a deprecation note), `docs/discord-integration.md`, and the Proxmox installer: fresh installs write the new names. Existing env files keep working through the fallback; the updater rewrites old names in place if that stays simple (`sed`), otherwise the warning tells operators to rename. Update `deploy/proxmox/test.sh`.
4. Run `mise run precommit` and `deploy/proxmox/test.sh`. In the portal, confirm the review service still boots, and confirm the warning appears when it runs with the old names.
<!-- SECTION:PLAN:END -->
