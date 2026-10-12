---
id: TASK-7
title: Show attributed kills in player rivalries
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-10-12 05:55'
updated_date: '2026-10-12 06:08'
labels: []
dependencies: []
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
With victim attribution now recorded, player rivalries should distinguish actual eliminations from merely winning a shared game. Count-only history must not fabricate opponent relationships.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Player head-to-head stats include directional attributed kills and respect the selected date range without counting unknown victims.
- [x] #2 Rivalries display separate kill-based leaders and head-to-head rows distinguish kills from game wins, with clear zero-data states.
- [x] #3 Backend and UI regression tests and full precommit pass; desktop and narrow layouts are exercised through the active portal.
- [x] #4 Rivalry callouts and counts drill into matching games in the selected range, with directional kill attribution and correct opposing winners/commanders.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extend existing player head-to-head aggregation with kills and killed_by from seat attribution. 2. Add two separate kill-leader callouts and directional counts to existing head-to-head rows without changing win/commander rankings. 3. Test asymmetric kill histories, unknown totals and date scopes; inspect populated and empty portal states, then run precommit.

4. User follow-up: make every rivalry callout and rivalry count open its matching games with the date range. Add directional kill filters and opponent commander/result filters; verify winner-versus-loser and same-seat semantics, URL chips, and real portal click-throughs.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added directional attributed kill counts, separate kill leaders, and player rivalry/head-to-head drilldowns with date-preserving filters and removable chips. Unknown totals never create attribution. Verified 748 Rust tests and 520 frontend tests with full mise run precommit (0 errors; 3 existing game-form test warnings), plus portal clicks for all six cards, commander table and head-to-head counts, date range changes, zero-result filters, and desktop/390px populated and empty layouts. Changes are local and uncommitted; review portal remains active.
<!-- SECTION:FINAL_SUMMARY:END -->
