---
id: TASK-3
title: Fix model-checked webcam rematch isolation bugs
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-10-09 07:00'
updated_date: '2026-10-09 07:10'
labels: []
dependencies: []
type: bug
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The user authorized fixes for the two TLC counterexamples in TASK-2: reused turn revisions and in-flight full-seat writes restoring pre-rematch life. Existing failing Rust regressions establish both bugs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Old turn revisions cannot advance a rematch and current turn commands still work
- [x] #2 Pre-reset seat writes cannot restore life or eliminate seats in a rematch; current-generation updates work
- [x] #3 Fixed TLC model satisfies both invariants and precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Keep turn revisions monotonic across rematches.
2. Add serde-default server-owned seat generation, reject stale writes, and keep status elimination atomic with the guarded write.
3. Extend existing regressions with current-generation and restored-session controls.
4. Retain original TLC counterexamples and add a fixed configuration; update documentation and run precommit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Turn revisions now advance across rematches. Seat snapshots persist a serde-default server-owned generation; stale writes return false and channel handlers report an error, then consume the queued reset. Optional status elimination is applied atomically with the guarded seat write. Expanded the existing two regressions to cover accepted current-generation updates, fresh pass/undo, stale elimination, restart persistence and a second rematch. Old TLC configs still reproduce both bugs; Fixed=TRUE passes all invariants across 566 states with two rematches/revisions 0–8. mise run precommit passes including 613 integration and 489 frontend tests, all other Rust suites, checks and build. Portal DOM smoke check passed: life 17→40 on rematch, camera toggle preserves reset, life 29 afterward, pass/undo returns active player. Added disposable local review players and restored dev account link to Rematch reviewer; no shared data changed. Review service stays running.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Both requested fixes implemented and verified locally. Regression tests and full precommit pass; fixed TLC model has no invariant violations within its stated bounds, while historical configs retain counterexamples. No commits or pushes made. Review portal: https://t-03h0lmi9hqfim0kpk44vyu1z2-p25422.onamp.dev/.
<!-- SECTION:FINAL_SUMMARY:END -->
