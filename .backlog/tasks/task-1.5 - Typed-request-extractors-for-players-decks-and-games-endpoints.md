---
id: TASK-1.5
title: 'Typed request extractors for players, decks, and games endpoints'
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:49'
updated_date: '2026-10-07 23:10'
labels: []
dependencies:
  - TASK-1.4
parent_task_id: TASK-1
priority: medium
type: enhancement
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Same problem as TASK-1.4, for the games domain: `web/api/games.rs` (players, decks, games, deck chooser, decklists, remote decks, admin player linking) takes `Params`, casts ids "like Ecto" (`cast_id`, `parse_id`), and passes `serde_json::Value` attrs into `games/{game,player,deck,resolve_player}.rs`, which cast them with `changeset.rs`. Game create and update carry a nested seats list whose per-row validation errors the SPA renders by index. `include_archived` only honors a JSON `true` in the merged params, which a GET query string can never carry.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Games-domain handlers and games/ domain functions take typed inputs; no serde_json::Value attrs or changeset casting remain there
- [x] #2 Seat row errors still render per row in the game form (vitest plus a portal check)
- [x] #3 include_archived works as a boolean query parameter (test)
- [x] #4 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. games/input.rs adds PlayerInput, DeckInput, SeatInput, and GameInput. All fields are Patch<T> with #[serde(default)], and the provenance fields (discord_id, source, external_id) are only set by trusted callers. The web handlers drop provenance from request bodies (member_player, GameInput::without_provenance). Patch gains nonblank() and From<Option<T>>.
2. Domain functions take the typed inputs and validate with Validator: game::validate_game (seats patch with per-row errors; the Phoenix keyed-map seat form is dropped), record_game create/update/upsert/find_or_create/portable, player create/update/find_or_create_player_by_name, deck create/update/validate_new_deck/find_or_create_deck, list_games(GameFilters), list_player_identities(IdentityQuery), and pick_deck(exclude_id: Option<i64>).
3. Internal callers build typed inputs instead of JSON: imports (commit, csv_transfer with Unchanged for blank file fields, sheet_commit, portable rows deserialized from the export with errors naming the record), Discord (sink, web_draft with a typed DraftResult for the result-draft body), sync_remote_decks, link_catalog_cards, and seed.
4. web/api/games.rs uses JsonBody, QueryParams, and PathParam everywhere: flat bodies; include_archived as a boolean query; typed MergeTarget, LinkPlayer, OutcomeBody, DeckListQuery, DeckDeleteQuery, and ChooserQuery; and V1Query validated into GameFilters. The decklist resolve and show handlers are typed too.
5. The SPA drops the {game|deck|player} wrappers (game form for both /api/games and the Discord result drafts, the deck editor, the new-commander dialog, retire deck, and buildGamePayload) along with the vitest expectations.
6. Updated the tests (support::input for domain inputs, ids() reading filters through axum's Query, numeric ids) and added an include_archived test. Ran mise run precommit and a portal walkthrough.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Scope note: the Discord result-draft endpoint (originally TASK-1.6) moved here because the game form posts the same body to it, so dropping the {"game": ...} wrapper had to change both. Decisions: ids and numbers in bodies and queries must be numbers (wrongly typed values, such as "abc" in a query or "12" as a JSON string id, are 400 instead of being cast or ignored). Out-of-range filter values (page=0, hour=25) are still ignored. include_archived=true is now a real query parameter; the SPA never used it, so the old JSON-true-only quirk had no visible effect. Blank strings still clear optional text through Patch::nonblank/trimmed. The CSV transfer keeps fields the file leaves blank (Patch::Unchanged). A deck_id choice in a sheet import that is not numeric is a deck_id "is invalid" error. Portable imports report wrongly typed export fields as "<Record>: <serde error>". Lotus gaps: none.

Validation: mise run precommit exit 0 (Rust 59 unit and 610 integration tests, vp check, 481 vitest tests, build). Portal stack (review service): the game form with Cody in both seats showed "cannot contain the same player twice" (task-1.5-duplicate-seat.png). A valid game against a new player (created through flat POST /api/players) was recorded as /games/41 with kills 2 and turns 8. Editing its notes on /games/41/edit saved. The deck editor renamed deck 8 and Retire deck archived it (then restored through the API). Merging Portal Tester into Jules from the player page moved the seat (game 41 seats: Cody, Jules). Row-error rendering per seat is covered by the game-form vitest suite, and the API 422 rows shape is covered by integration tests.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Players, decks, and games take typed inputs end to end: games/input.rs (PlayerInput, DeckInput, SeatInput, GameInput with Patch fields), GameFilters, and IdentityQuery. Every handler uses the JSON-rejecting extractors with flat bodies, and every internal caller (imports, Discord, seed, deck sync) builds typed inputs instead of JSON. include_archived is a boolean query parameter, and the SPA sends flat bodies. Verified with mise run precommit and a portal walkthrough of game record/edit, the duplicate-seat error, deck rename/retire, and player merge.
<!-- SECTION:FINAL_SUMMARY:END -->
