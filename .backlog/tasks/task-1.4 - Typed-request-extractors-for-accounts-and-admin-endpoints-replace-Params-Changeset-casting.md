---
id: TASK-1.4
title: >-
  Typed request extractors for accounts and admin endpoints (replace
  Params/Changeset casting)
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:48'
updated_date: '2026-10-07 22:45'
labels: []
dependencies:
  - TASK-1.1
  - TASK-1.3
parent_task_id: TASK-1
priority: medium
type: enhancement
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Handlers take `web::params::Params`, a Plug.Parsers emulation that merges the query string and a JSON or urlencoded body into one `serde_json::Value` (with `a[b]=c` nesting and `_json` wrapping). Domain functions then cast fields out of that `Value` with `changeset.rs`, which applies Ecto's rules: blank strings become nil, numeric strings become integers, and "1"/"0" become booleans. Request bodies use Phoenix resource wrappers (`{"user": {...}}`, `{"settings": {...}}`, `{"api_key": {...}}`). In idiomatic axum, handlers take typed `Json<T>`, `Query<T>`, and `Path<T>` extractors whose rejections render the API's JSON errors, and domain functions take typed inputs. This subtask covers the accounts and admin surface and introduces the shared pieces the next two subtasks reuse. The SPA forms and their tests change in the same commit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Accounts and admin handlers take typed Json/Query/Path inputs; their domain functions no longer take serde_json::Value params or use changeset casting
- [x] #2 Malformed or wrongly typed bodies get a JSON 400 (or 413/415) through ApiError, never axum plain-text rejections (tests)
- [x] #3 PATCH endpoints distinguish absent, null, and value fields (tests)
- [x] #4 The SPA sends the new request shapes; vitest and a portal check of sign-in, registration, profile, appearance, password, API keys, and admin edits pass
- [x] #5 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Shared pieces: web/extract.rs has JsonBody, QueryParams, and PathParam (derived via axum's FromRequest/FromRequestParts with an ApiError rejection), and error.rs converts Json, Query, and Path rejections (400 for malformed input or wrong types; 413 and 415 keep their status). patch.rs has Patch<T> (Unchanged or Set(Option<T>), serde with #[serde(default)]) plus trimmed(), where a blank string clears. Patch replaces changeset::Change.
2. Typed inputs in accounts/user.rs: NewAccount, ProfileUpdate, AppearanceUpdate, PasswordUpdate, AccountUpdate (disabled: bool instead of disabled_at), SettingsUpdate, and NewApiKey. The handlers have small typed structs: InviteToken, Credentials, Reauthentication, DiscordRequest, and DiscordCallback. Domain functions in accounts/mod.rs take these and validate with Validator. Accounts no longer use Changeset.
3. Flat request bodies with no Phoenix wrappers. The SPA callers are updated (settings, theme, admin settings and users, API keys, registration) along with their vitest expectations, plus the README appearance example and AGENTS.md conventions.
4. Integration tests now send flat bodies, build domain inputs with support::input, and cover 415 for form bodies, 413 for oversized bodies, 400 for path ids and wrong types, and Patch semantics for the profile (absent, null, blank). Ran mise run precommit and portal checks.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions: request bodies are flat; the {"user"|"settings"|"api_key": ...} wrappers are gone and the SPA is updated in this commit. Missing required fields stay 422 validation errors (inputs use Option fields); wrong JSON types are 400; non-JSON bodies are 415 (form posts to /api/session are no longer accepted). Required credentials (session create, sudo, password) are plain String fields, so a missing key is 400 rather than the old 401. Invite tokens must be strings: null or an object gives 400, while string tokens still answer valid:false and clear the pending invite. Admin user updates take disabled: true/false only (no raw disabled_at), and re-disabling keeps the original disabled_at. detailed_stats_from takes YYYY-MM-DD or null; the SPA already sends null to clear. PATCH semantics come from Patch<T>, and blank deck-host fields clear explicitly through trimmed(). Removed handlers no longer log params at debug level; the generic Params logging goes away in TASK-1.6. Lotus gaps: none.

Validation: mise run precommit exit 0 (Rust 59 unit and 609 integration tests; vp check; 481 vitest tests; build). Portal stack: a separate tg-auth service (no dev auto-login, fresh DB, Vite 5174, stopped afterwards) covered bootstrap registration as owner; password change (mismatch error shown, then "Password updated"); logout; login rejected with the old password and accepted with the new one; API key creation (the key then authenticated GET /api/v1/games with 200); admin settings toggle of open registration (persisted) plus detailed_stats_from set and cleared; admin user page showing a too-short username error, a rename to Head Owner, and the last-admin disable error. Artifacts: task-1.4-password-updated.png, task-1.4-admin-settings.png, task-1.4-admin-users.png. Review service healthy after restart.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Accounts and admin endpoints take typed inputs (JsonBody/QueryParams/PathParam with JSON rejections) and flat request bodies, and domain functions take typed structs validated by Validator. Patch<T> distinguishes absent, null, and value fields, and the SPA sends the new shapes. Verified with mise run precommit, new tests for 400/413/415 and Patch semantics, and a browser walkthrough of registration, password change, login, API keys, and admin edits.
<!-- SECTION:FINAL_SUMMARY:END -->
