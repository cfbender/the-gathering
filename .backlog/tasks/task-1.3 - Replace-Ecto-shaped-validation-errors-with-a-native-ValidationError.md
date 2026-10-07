---
id: TASK-1.3
title: Replace Ecto-shaped validation errors with a native ValidationError
status: To Do
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:48'
updated_date: '2026-10-07 21:48'
labels: []
dependencies: []
parent_task_id: TASK-1
priority: medium
type: enhancement
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`error::Errors` renders a field's messages newest first and lets per-row errors replace a list's own messages, both to match `Ecto.Changeset.traverse_errors/2`. The validation half of `changeset.rs` (`validate_required`/`length`/`format`/`inclusion`/`number` equivalents with Ecto's wording) is mixed with Ecto-style casting. `imports/inspect.rs` renders Elixir `inspect/1` syntax (`%{name: ["can't be blank"]}`, `nil`) into user-facing import messages. The `{"errors": {"field": ["message"]}}` 422 body with nested row arrays is the documented API convention, which the SPA's `ApiError.fieldErrors` reads, so it stays. Only the order, the nesting semantics, the Rust type, and the human-readable rendering become ours. Casting is out of scope; the typed-request subtasks replace it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One ValidationError type (insertion-ordered field messages, nested row errors, readable Display) replaces error::Errors and the validate_* half of changeset.rs
- [ ] #2 The 422 JSON shape {"errors": {...}} is unchanged, and the SPA shows field and row errors as before (vitest plus a portal check)
- [ ] #3 Import errors no longer contain Elixir inspect syntax (%{...}, nil, atom keys); tests cover CSV, Mythic Track, and portable import messages
- [ ] #4 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add `validation.rs`: `ValidationError` (field messages in insertion order, plus nested row errors kept alongside the list's own messages under a documented rule) with `Display`, which gives a readable sentence ("Name can't be blank; seat 2: player has already been taken") for logs and import messages. Add a `Validator` builder that carries the existing checks (required, length, max bytes, format, inclusion, number bounds, taken/does-not-exist) and returns `Result<T, ValidationError>`.
2. Replace `error::Errors` with `ValidationError` everywhere (`ApiError::Validation`, accounts, games, imports, discord). Keep the JSON shape `{"errors": {field: [messages], rows_field: [{}, {field: [..]}]}}`. Message wording stays; it is ordinary English and the SPA shows it verbatim.
3. Replace `imports/inspect.rs` renderings with `ValidationError`'s `Display` and plain-text values (`null` or empty for absent values) in import preview and commit errors. Update the affected import tests and any SPA expectations of those strings.
4. Update the `lib/api.ts` comments (they cite `TheGatheringWeb.ChangesetJSON`) and the `ApiError.fieldErrors` tests, adjusting them if order-dependent assertions change. Update the AGENTS.md JSON convention note if anything changes.
5. Run `mise run precommit`. In the portal, trigger a multi-error validation (for example, registration with a short password and a taken username, or a game with a duplicate seat) and confirm the form shows the messages.
<!-- SECTION:PLAN:END -->
