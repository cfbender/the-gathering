---
id: TASK-1.4
title: >-
  Typed request extractors for accounts and admin endpoints (replace
  Params/Changeset casting)
status: To Do
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:48'
updated_date: '2026-10-07 21:49'
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
- [ ] #1 Accounts and admin handlers take typed Json/Query/Path inputs; their domain functions no longer take serde_json::Value params or use changeset casting
- [ ] #2 Malformed or wrongly typed bodies get a JSON 400 (or 413/415) through ApiError, never axum plain-text rejections (tests)
- [ ] #3 PATCH endpoints distinguish absent, null, and value fields (tests)
- [ ] #4 The SPA sends the new request shapes; vitest and a portal check of sign-in, registration, profile, appearance, password, API keys, and admin edits pass
- [ ] #5 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Shared pieces: an `ApiJson<T>`/`ApiQuery<T>`/`ApiPath<T>` wrapper (or `WithRejection`) that maps axum rejections to `ApiError` (400 for malformed JSON or wrong types, 413 for too-large bodies, 415 for a non-JSON content type), and a `Patch<T>` (absent, null, or value) for PATCH fields, replacing `changeset::Change`. Decide per field whether blank strings mean "clear". Where the SPA relies on that, normalize explicitly in the input type rather than through a global cast rule.
2. Convert these handlers in `web/api/accounts.rs` and their domain functions in `accounts/{mod,user,discord}.rs` to typed inputs: session create, registration (and the bootstrap admin), registration-invite create, profile and appearance and password update, API key create, admin users update, admin settings update, and admin invite. Validation goes through `Validator` (from TASK-1.3).
3. Request bodies: drop the Phoenix resource wrappers (`{"user": {...}}` becomes `{...}`) and update the SPA callers (`routes/settings.tsx`, `lib/theme.tsx`, `features/admin/admin-pages.tsx`, `features/api-keys`, login and registration forms) and their tests. If the owner vetoes this, keep the wrappers as typed envelope structs.
4. Update the integration tests (accounts, auth_api, admin_users_api, api_key_api, registration_invite_api, dev_auto_login) for the new bodies and the rejection statuses. Run `mise run precommit`. In the portal, check sign-in, registration with an invite, profile, appearance, and password changes, API key creation, and admin user and settings edits.
<!-- SECTION:PLAN:END -->
