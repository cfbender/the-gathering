---
id: TASK-8
title: Record kills and MVP cards in the post-game modal
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-10-12 06:29'
updated_date: '2026-10-12 06:38'
labels: []
dependencies: []
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Live-table results currently omit per-player kill attribution and MVP cards, requiring a second edit after saving. Bring the same optional outcome logging into the post-game flow.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Each seated player can record named victims, optional total kills, and an optional MVP card in the post-game modal.
- [x] #2 Record result and Record and rematch preserve those fields, with one killer per victim and no automatic attribution from winning.
- [x] #3 Targeted tests, full precommit, and desktop/narrow portal checks verify the modal and persisted game details.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Reuse the game form kill picker and card search; share victim/total update logic. Map per-player results to existing game API fields. Verify save, rematch and blank defaults through tests and the live portal.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Validation: targeted tests 35 passed; full mise run precommit passed (748 Rust tests, 523 frontend tests, production build, formatting/lint/typecheck; three existing frontend warnings). Live two-player portal verified named kill and Sol Ring MVP persisted via Record and rematch (game 43), blank rematch defaults, then a number-only winner kill with no inferred victim and a losing-player MVP via Record result (game 44). Desktop, open victim dropdown and 390px narrow screenshots inspected; narrow dialog has no horizontal overflow. Portal remains running. Modal changes are local and uncommitted; preceding rivalries work was pushed separately.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added optional per-player victims, kill totals and MVP cards to post-game recording using existing shared controls and API fields. Both close and rematch save paths verified in tests and the live portal, with database persistence and no inference for number-only kills. Full precommit passed.
<!-- SECTION:FINAL_SUMMARY:END -->
