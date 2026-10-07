---
id: TASK-1.3
title: Replace Ecto-shaped validation errors with a native ValidationError
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:48'
updated_date: '2026-10-07 22:29'
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
- [x] #1 One ValidationError type (insertion-ordered field messages, nested row errors, readable Display) replaces error::Errors and the validate_* half of changeset.rs
- [x] #2 The 422 JSON shape {"errors": {...}} is unchanged, and the SPA shows field and row errors as before (vitest plus a portal check)
- [x] #3 Import errors no longer contain Elixir inspect syntax (%{...}, nil, atom keys); tests cover CSV, Mythic Track, and portable import messages
- [x] #4 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add src/validation.rs. ValidationError keeps field messages in check order plus per-row errors for list fields and renders the same 422 JSON shape. A list's rows win over its own messages in JSON so row indexes match the input; Display includes both as readable sentences ("Name can't be blank; Seat 2: Player has already been taken"). Validator carries the checks (required, length, max bytes, format, inclusion, number bounds) together with the TAKEN and DOES_NOT_EXIST messages.
2. error.rs drops the Errors type and ApiError::Validation holds a ValidationError. changeset.rs keeps only casting (transitional); Changeset derefs to its Validator so call sites are unchanged until typed inputs replace it. Validation-only uses (Discord sign-in, player resolution, account field checks) take a Validator directly.
3. Delete imports/inspect.rs. Import errors use ValidationError's Display, the Mythic Track status shows the JSON value (status null, status "done"), and CSV escape errors quote lines with Rust's Debug.
4. Update the lib/api.ts comments, the AGENTS.md JSON conventions, and the rust/README layout. The SPA reads fields by key, so no logic changes.
5. Tests: validation unit tests (order, rows, Display, Validator), the seat rule order, csv_transfer and portable message tests, and a Mythic Track unknown-status integration test. Ran mise run precommit and a portal check of profile field errors plus a game seat-row 422.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions: kept the documented 422 JSON shape. Messages within a field now appear in check order instead of newest first. A list field with row errors still renders only its rows in JSON (the SPA numbers rows by index); its own messages, such as "cannot contain the same player twice", are kept in Display and logs. The live check shows that JSON drops them, as it did before; changing that would need a new JSON shape and SPA support, so it was not done here. Changeset now wraps a Validator (Deref) until TASK-1.4 to 1.6 replace casting with typed inputs. Import error messages no longer use Elixir inspect syntax. Lotus gaps: none.

Validation: mise run precommit exit 0 (56 unit and 607 integration tests, vp check, vitest, build). Portal stack: on Settings > Profile, an invalid Moxfield name and a ManaVault URL with a path showed both field messages under their inputs (artifact task-1.3-profile-validation.png). POST /api/games with a duplicate player and negative kills returned 422 {"errors":{"seats":[{"kills":["must be greater than or equal to 0"]},{}]}}.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the Ecto-shaped error::Errors with validation::ValidationError (check-ordered field messages, per-row list errors, readable Display) and a Validator for the checks. The 422 JSON shape is unchanged, and import errors read as plain sentences instead of Elixir inspect output. Verified with mise run precommit, new unit and integration tests, and a portal check of field errors.
<!-- SECTION:FINAL_SUMMARY:END -->
