---
id: TASK-11
title: Multiple playgroups with a header switcher
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-11-07T20:19:00Z'
updated_date: '2026-11-07T20:59:00Z'
labels: []
dependencies: []
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
One server currently holds one circle of players, decks, and games. Admins want to host more than one playgroup (for example a second group of friends) on the same server without the data mixing: each playgroup owns its players, decks, games, imports, and stats, and the app shows one playgroup at a time. A switcher in the app header shows the active playgroup and changes it. Existing data moves into a single playgroup created by the migration so upgrades are transparent.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A migration creates the playgroups table, moves every existing player and game into one default playgroup, and keeps upgrades of existing databases transparent
- [x] #2 Players, decks, games, stats, imports, and player identity admin only read and write the active playgroup; by-id lookups of rows in another playgroup return 404
- [x] #3 Player name, Discord id, and account links are unique per playgroup, so one account can be a player in several playgroups
- [x] #4 Admins can create, rename, and delete (only when empty, never the last) playgroups through /api/playgroups and an admin page
- [x] #5 Every signed-in user can switch the active playgroup from the app header; the choice persists on the account and the server's SPA shell and session JSON expose it
- [x] #6 Discord-recorded games land in the playgroup linked to the guild, or the default playgroup when none is linked
- [x] #7 Rust integration tests and frontend tests cover scoping, switching, and playgroup administration; precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Migration: playgroups table (name unique nocase, optional discord_guild_id), rebuild players and games with NOT NULL playgroup_id (default playgroup 'Main' created from existing data), per-playgroup unique indexes (name, discord_id, user_id; games source+external_id), users.playgroup_id (ON DELETE SET NULL), recreate audit triggers for players/games with playgroup_id.
2. Backend: playgroups module + Playgroups pool API on AppState (list/get/create/update/delete, default_id, for_discord_guild); User.playgroup_id resolved as COALESCE(users.playgroup_id, min playgroup) in select_users!; Player/Game carry playgroup_id.
3. Thread playgroup_id through games::{player,deck,resolve_player,record_game,list_games,merge_players,deck_picker}, stats::query, imports, discord sink/web_draft, webcam seat lookups, seed.
4. API: /api/playgroups (list for members; create/patch/delete admin+sudo), PATCH /api/session/playgroup; session JSON and SPA shell expose playgroup_id; existing handlers use user.playgroup_id.
5. Frontend: playgroups query + PlaygroupSwitcher in the header (desktop and mobile), invalidate resource queries on switch, admin Playgroups page.
6. Tests: Rust integration (scoping, 404 across playgroups, switching, admin CRUD, Discord guild routing); vitest for switcher and admin page; mise run precommit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions: every member sees every playgroup (no membership model); the migration names the default playgroup 'Main' (id 1, the oldest); the active playgroup is users.playgroup_id resolved as COALESCE(users.playgroup_id, min(id)), so deleting a playgroup drops its viewers back to the default; admin CRUD lives under /api/admin/playgroups (sudo) while GET /api/playgroups and PATCH /api/session/playgroup are member routes; a playgroup can claim one Discord guild and unclaimed guilds record into the default; portable_id stays globally unique so re-importing an export into a second playgroup reports portable_id taken; webcam table player/deck lookups stay unscoped (they check ownership) while POST /api/games uses the submitter's playgroup. The SPA shell does not render playgroup_id (nothing needs it before the session query). Frontend: PlaygroupSwitcher (DropdownMenu radio list next to the logo at every width; admins get a Manage playgroups link) invalidates every query except session and playgroups on switch; admin page at /admin/playgroups with create, inline edit, confirm delete, sudo prompt. Header nav padding dropped from px-3.5 to px-3 so the wordmark, switcher, nav, theme toggle and user menu fit at 1280px. Verification: mise run precommit passed (686 integration + 92 lib Rust tests incl. new tests/integration/playgroups_api.rs; 575 vitest tests incl. playgroup-switcher.test.tsx and playgroups-page.test.tsx); migration applied to the dev database at boot; portal checked at 1280 and 390 widths (switcher, menu, switching to an empty playgroup, admin create/edit/list).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added playgroups: migration 20261107202025 creates the table, moves existing players and games into 'Main', scopes players/games/decks/stats/imports/identity admin by playgroup with per-playgroup unique indexes, and records the active playgroup on users. New src/playgroups.rs + web/api/playgroups.rs (list, switch, admin CRUD), User.playgroup_id in session JSON, Discord guild routing. Frontend: header PlaygroupSwitcher, admin Playgroups page and tab, README docs. Risks: portable exports cannot be re-imported into a second playgroup (portable_id is global); all members can see and switch to every playgroup.
<!-- SECTION:FINAL_SUMMARY:END -->
