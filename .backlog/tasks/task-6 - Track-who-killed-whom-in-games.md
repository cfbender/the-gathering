---
id: TASK-6
title: Track who killed whom in games
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-10-12 05:05'
updated_date: '2026-10-12 05:16'
labels: []
dependencies: []
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Kill totals cannot identify victims. Add victim selection while preserving count-only history and forgotten identities, using the existing eliminated-by relationship.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Game create and edit support victim checkboxes and count-only or partially known totals, including Discord drafts.
- [x] #2 Existing sole-winner sweeps are backfilled without overwriting attribution; ambiguous counts remain unchanged.
- [x] #3 Game details display named victims and unknown remainder; invalid attribution is rejected.
- [x] #4 Targeted tests, precommit checks, and review portal verification pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reuse eliminated_by_player_id and validate seated, non-self attribution and totals. Add conservative historical backfill. 2. Add draft-aware checkbox picker, Discord identity mapping, and game detail display. 3. Verify migrations, create/edit behavior, tests and portal states.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reused eliminated_by_player_id; no new schema column or portable-export version. Historical migration attributes sole-winner n-1 sweeps only when existing attribution does not conflict; new count-only saves remain unattributed. Checkbox selections consume unknown totals before incrementing; unchecking reduces the total. Player replacement/removal clears attribution rather than assigning it to another identity. API and CSV corrections reject totals below named victims; adjusted inconsistent import fixtures and verified conflict rejection without writes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented checkbox kill tracking, optional totals, game-detail victim display, Discord draft resolution, and conservative historical backfill. mise run rust:sqlx-prepare completed with unchanged query metadata. mise run precommit passed: 746 Rust tests, 517 frontend tests, formatting, lint/type checks, and production build. Portal create/edit/reorder/reload confirmed persistence, count-only remainder, zero vs unknown, and disabled duplicate attribution. Desktop and 390px Chromium screenshots inspected in .amp/in/artifacts/kills-*.png. Review service remains active. Changes remain uncommitted and unpushed.
<!-- SECTION:FINAL_SUMMARY:END -->
