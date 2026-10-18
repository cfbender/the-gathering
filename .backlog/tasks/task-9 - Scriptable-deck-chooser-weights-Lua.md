---
id: TASK-9
title: Scriptable deck chooser weights (Lua)
status: Done
assignee: []
created_date: '2026-10-18 05:36'
updated_date: '2026-10-18 06:34'
labels: []
dependencies: []
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The deck chooser uses one fixed formula (recency × (skips + 1) ÷ (plays + 1), with a boost for never-played decks). Members want different weighting: boosting decks they lost with quickly (in case someone got stomped), more color variety, or weighting by day of week, time of night, or how many games they have played that day. Not everyone wants the same rules, so instead of adding toggles each member can write a small sandboxed Lua script that returns a weight per deck. Members without a script keep the current formula.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A member can save, clear, and reset their own deck chooser script; saving rejects scripts that fail to compile with a field error
- [x] #2 With a script saved, GET /api/deck-chooser picks using the weights the script returns; weight 0 excludes a deck; members without a script get the existing formula unchanged
- [x] #3 Scripts see each deck (colors, plays, skips, wins, losses, recent games with duration, turns, and result, plus the built-in weight) and context (local time in the browser zone, games played today, recent games across decks)
- [x] #4 Scripts run sandboxed: no io/os/package/debug/load, with instruction and memory limits, off the async runtime
- [x] #5 A script that errors or returns an invalid weight at pick time falls back to the built-in weights and the chooser shows the error
- [x] #6 A settings editor previews each decks chance under the draft script before saving, and ships examples (quick-loss boost, color variety, time of night, games today)
- [x] #7 Rust integration tests and frontend tests cover the contract; docs describe the script API
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Backend runtime: mlua (Lua 5.4, vendored) in games/deck_script.rs. Fresh state per run with string/table/math/utf8 and the safe base library (no load/loadfile/dofile/require/collectgarbage/string.dump, no io/os/debug/package), 32 MB memory, 50M instructions, 500 ms, print captured to a log, run in spawn_blocking with a 2 s request timeout.
2. Script contract: global weight(deck, ctx) returns a number >= 0 (0 excludes) and an optional note. games/chooser_context.rs builds deck/game/opponent/time/ctx tables from the playable decks and full game history in the browser zone.
3. Storage: users.deck_chooser_script (migration 20261018054335), schema.sql and .sqlx regenerated.
4. API: GET/PUT /api/deck-chooser/script; POST /api/deck-chooser/preview returns each deck input, weight, chance, note, the ctx, logs, and error; GET /api/deck-chooser takes tz and adds chance, note, script_error (fallback to built-in weights on any script failure, including all-zero weights).
5. Frontend: /decks/choose/script editor (line numbers, Tab indent, error line), examples menu, live debounced preview (context summary, per-deck recency/plays/skips/W-L-D/weight/chance with expandable Lua tables, log), Context and Reference tabs (reference parsed from docs/deck-chooser-scripts.md); Customize link, note, chance, and fallback warning on the chooser; note on the webcam table chooser.
6. Tests and docs: Rust unit and integration tests (sandbox, limits, errors, picks, fallback, preview fields in a non-UTC zone, every example runs, every field documented); Vitest for editor, preview, save errors, reference parsing, chooser warning; docs/deck-chooser-scripts.md, README, AGENTS.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Validation: mise run rust:check passed (unit + 644 integration tests incl. deck_chooser_scripts), aube run precommit passed (vp check, 535 Vitest tests, build). Portal checks: examples menu, live preview with notes, expanded deck table, runtime error with line marker and fallback, Context and Reference tabs, save, chooser note/chance, chooser fallback warning, reset confirm, 390 px layout. Pattern matching (string.find/gsub) runs in C and is not interrupted by the instruction hook; the 2 s request timeout bounds the request but not the worker thread.

Follow-up: palette-aware Lua syntax highlighting (tokenizer in features/decks/lua-highlight.tsx, .lua-* colors in app.css mixed from theme tokens, transparent-textarea overlay) in the editor and the inspected tables; Settings gains a Deck chooser section linking to the editor. aube run precommit passed (537 tests).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Members can weight the deck chooser with a sandboxed Lua script (weight(deck, ctx)) edited at /decks/choose/script with a live preview against their real decks and history, ten examples, and an in-app field reference parsed from docs/deck-chooser-scripts.md. Picks fall back to the built-in weights with a visible error when a script fails. Verified with Rust unit/integration tests, Vitest, and portal walkthroughs.
<!-- SECTION:FINAL_SUMMARY:END -->
