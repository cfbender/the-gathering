---
id: TASK-5
title: Allow table hosts and admins to remove participants
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-10-11 03:32'
updated_date: '2026-10-11 03:41'
labels: []
dependencies: []
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Hosts and admins need to remove unwanted players or spectators without ending the table for everyone.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Only hosts and admins can remove another participant, including spectators
- [x] #2 Removed participants disconnect and cannot rejoin the same table; started game results remain recordable
- [x] #3 Confirmation controls and removal feedback are tested and verified through the review portal
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add durable room removal and channel authorization; expose moderation permission and confirmed participant controls; test authorization, reconnect denial and cleanup; run precommit and exercise the review portal.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented confirmed host/admin removal for players and spectators. Removal is persisted per table, blocks reconnects and rematches, stops client media, and preserves started-game results and team positions. Verified with mise run precommit: 744 Rust tests and 505 frontend tests passed, clean lint/type checks, production build passed. Exercised member permission hiding, cancel/confirm, player and spectator removal, redirect notice and rejected rejoin through the supervised review portal; inspected desktop and narrow screenshots. Changes remain local and uncommitted.
<!-- SECTION:FINAL_SUMMARY:END -->
