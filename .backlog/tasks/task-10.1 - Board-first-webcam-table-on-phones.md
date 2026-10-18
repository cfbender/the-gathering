---
id: TASK-10.1
title: Board-first webcam table on phones
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-18 15:18'
updated_date: '2026-10-18 16:02'
labels: []
dependencies: []
parent_task_id: TASK-10
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Below the lg breakpoint the table stacks the desktop grid: a rail of fixed 240px video tiles that overflows, the active board squeezed between it and a side panel taking up to 45dvh, and the active seat bar shown twice. In landscape the active board collapses to nearly nothing. Players on phones cannot see boards, and the virtual board will need the same screen.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 On a 390x844 portrait phone the active board takes most of the screen height with no horizontal page overflow
- [x] #2 On an 844x390 landscape phone the active board stays visible and usable
- [x] #3 Every seat is reachable on a phone through a compact seat switcher showing name, life, current turn, and camera state
- [x] #4 The table panel on phones opens on demand and starts closed, and every tab remains reachable
- [x] #5 Your own life and Pass turn are reachable on a phone without opening the panel
- [x] #6 Desktop (lg and up) layout is unchanged
- [x] #7 Frontend checks and tests pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Below lg, replace the 240px camera rail with a seat-chip strip (name, life, turn, camera off) that pins boards like the rail.
2. Add a thumb bar with your life -/+, Pass turn (after start, seated only), and a panel button; landscape phones put the strip and bar in one row above the board.
3. Show the side panel on phones as a bottom sheet with a backdrop, closed by default (matchMedia lg check).
4. Keep lg+ layout identical; verify with before/after desktop screenshot diff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in table-mobile.tsx (MobileSeatStrip, MobileActionBar) plus layout classes in webcam-table-page, side-panel, table-camera-rail, table-stage. Verified in the review portal at 390x844 and 844x390, dark and light themes; desktop 1440x900 pixel diff only differs in the running clock. Frontend precommit passes (538 tests). Follow-up ideas not in scope: swipe between boards, pinch-zoom on the active board.

Camera-off chip icon and chip life confirmed with a scratch jsdom test (removed).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Phones get a board-first table: seat chips replace the camera rail, a thumb bar holds life, Pass turn, and the panel button, and the panel is a bottom sheet that starts closed. Verified in the review portal at 390x844 and 844x390 in dark and light themes (recording and screenshots in the design thread), desktop pixel diff unchanged apart from the clock, and frontend precommit (538 tests) passing.
<!-- SECTION:FINAL_SUMMARY:END -->
