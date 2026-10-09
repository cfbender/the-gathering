---
id: TASK-2
title: Model-check webcam turn and rematch invariants
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-10-09 06:28'
updated_date: '2026-10-09 06:32'
labels: []
dependencies: []
type: spike
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Use TLC to explore short state-machine traces and distinguish model counterexamples from bugs reproduced against the Rust implementation. Scope is webcam turns and room/rematch boundaries, not a proof of the entire application.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Runnable TLA+ models state their implementation mapping and finite bounds
- [x] #2 Every reported bug has a deterministic Rust regression reproduction
- [x] #3 Record TLC results, test results, and modeling limitations
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Read room, turn, channel and existing test contracts.
2. Run bounded TLC models of turns and rematch/message freshness.
3. Reproduce counterexamples with focused Rust tests; keep investigation separate from production fixes.
4. Run targeted tests and precommit; document exact reproduction commands and limits.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a bounded rematch-isolation model (two Commander seats, one rematch, revisions 0–4). TLC structural run exhausts 203 states; NoCrossGamePass and ResetLifePreserved each produce a counterexample. Rust reproductions confirm both via Socket.IO and the real room actor/persisted session. Production behavior intentionally unchanged for the requested audit; regression assertions remain enabled and red. mise run precommit passed schema/fmt/clippy, then integration tests returned 611 passed and exactly the two new failures; later frontend checks were not reached. Existing deps/ worktree content was left untouched.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Audit complete, bugs not fixed. Reproducible TLA+ model/configurations and documentation live in rust/models/. Two deterministic regression tests reproduce cross-rematch revision reuse and in-flight full-seat writes restoring old life after reset. TLC and Rust failure evidence agree. Changes remain local and uncommitted; tests/precommit intentionally fail until repairs are implemented.
<!-- SECTION:FINAL_SUMMARY:END -->
