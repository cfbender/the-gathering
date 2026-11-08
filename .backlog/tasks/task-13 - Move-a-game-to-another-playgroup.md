---
id: TASK-13
title: Move a game to another playgroup
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-11-08T01:46:00Z'
updated_date: '2026-11-08T02:00:00Z'
labels: []
dependencies: []
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
With private playgroups (TASK-12) a game can land in the wrong one: the Discord bot records an unclaimed server into the oldest playgroup, and members sometimes log a game while viewing the wrong playgroup. Deleting and re-entering loses seats, MVP cards, and kills. Administrators need to move a game between playgroups. Players and decks are per playgroup (decks hang off players), so a move has to find or create each seat's player and deck in the destination.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 An administrator with recent sudo can move a game from the viewed playgroup to another playgroup; the game, its seats, kills, eliminations, MVP cards, and notes are intact afterwards and it no longer appears in the source playgroup
- [x] #2 Each seat is re-pointed to the destination player that matches by linked account, then Discord id, then case-insensitive name; a missing player is created with the same name and identity, and a name match whose identity conflicts with the seat's player is a validation error
- [x] #3 Each seat's deck is re-pointed to the destination player's deck with the same name, or a copy of the deck is created there
- [x] #4 Moving to the game's current playgroup, an unknown playgroup, or a playgroup that already holds a game with the same source and external id is a validation error; non-admins and admins without sudo are refused
- [x] #5 The game detail page offers administrators a Move control listing their other playgroups with a confirmation, prompts for sudo when needed, and returns to the games list after moving
- [x] #6 Integration tests cover the remapping rules and refusals; vitest covers the move control; README mentions the option
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Backend: games/move_game.rs (move_game: validates destination, maps each seat to the destination player by user_id → discord_id → name NOCASE, creating it when missing and carrying a missing identity onto a name match; deck by name NOCASE else copied with skip_count 0; remaps eliminated_by_player_id; sets games.playgroup_id; unique violation on games.external_id → 422 'already has this game'). Games::move_game wraps it in write_tx!. Route POST /api/admin/games/{id}/move in the sudo-admin group (web/mod.rs), handler games_move in web/api/games.rs. Tests: tests/integration/games_move_api.rs (4 tests: remapping + deck copy, identity carry-over and conflict, destination refusals, auth/sudo). Frontend: features/games/move-game.tsx (card below the game detail, admins only, options from GET /api/playgroups minus the viewed one, ConfirmDialog, SudoPrompt retry, field error display) + move-game.test.tsx (4 tests); moveGame in games.ts; mounted in routes/games.$gameId.tsx. README: paragraph under Playgroups. Verified through the review portal: moved demo game #1 Main → Thursday Night Commander (players/decks created there) and back (seats re-attached to the original Main players/decks). mise run precommit green (695 integration, 92 unit, 580 vitest).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Admins can move a game to another playgroup they belong to from the game page (sudo). Seats are matched to the destination's players by account, Discord id, or name and to same-named decks, creating/copying what is missing; conflicts and duplicate imports are refused with 422. Backend + integration tests, React card + vitest, README.
<!-- SECTION:FINAL_SUMMARY:END -->
