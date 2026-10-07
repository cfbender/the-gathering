---
id: TASK-1.5
title: 'Typed request extractors for players, decks, and games endpoints'
status: To Do
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:49'
updated_date: '2026-10-07 21:49'
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
- [ ] #1 Games-domain handlers and games/ domain functions take typed inputs; no serde_json::Value attrs or changeset casting remain there
- [ ] #2 Seat row errors still render per row in the game form (vitest plus a portal check)
- [ ] #3 include_archived works as a boolean query parameter (test)
- [ ] #4 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Typed inputs for players (create, update, merge, admin link and unlink), decks (create, update, archive), games (create and update with a typed `Vec<SeatInput>`, list filters as `Query<GameFilters>`), the deck chooser outcome, and decklist resolve. Path ids are `Path<i64>` through the JSON-rejecting wrapper.
2. Domain functions in `games/` take typed structs and validate with `Validator`, keeping per-seat row errors in the 422 body.
3. `include_archived` becomes a real boolean query parameter (`?include_archived=true`). Check whether the SPA needs archived players or decks anywhere it currently cannot get them, and report that as a behavior fix.
4. Drop the resource wrappers (`{"player": {...}}`, `{"deck": {...}}`, `{"game": {...}}`), then update `features/games`, `features/decks`, the game form, and their tests.
5. Update the integration tests (games, games_api, players_api, decks_api, admin_players_api, decklists, stats where they create data). Run `mise run precommit`. In the portal, record a game with a duplicate seat (row error shown) and then a valid game, edit and archive a deck, and create and merge players.
<!-- SECTION:PLAN:END -->
