---
id: TASK-1.6
title: >-
  Typed extractors for the remaining endpoints, remove Params/Changeset, and
  native router/logging behavior
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:49'
updated_date: '2026-10-07 23:23'
labels: []
dependencies:
  - TASK-1.5
parent_task_id: TASK-1
priority: medium
type: enhancement
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The remaining `Params` users (imports, cards and printings, stats, Discord result drafts and pending games, card-recognition corrections, webcam) still use the Plug.Parsers emulation, and the HTTP layer keeps other Phoenix router behavior. A method mismatch answers 404 (`method_not_allowed_fallback(not_found)`), PATCH routes are also mounted as PUT because Phoenix `resources` generates both, `request_id.rs` reproduces Plug.RequestId and Phoenix.Logger's "Sent 200 in 5ms" lines, and `params.rs` logs every request's merged params at debug level with Phoenix's `filter_parameters`. Once this subtask lands, `web/params.rs` and the casting half of `changeset.rs` can be deleted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 web/params.rs and changeset.rs are deleted; no handler takes merged query-and-body params
- [x] #2 API method mismatches answer 405 with an Allow header; PUT aliases of PATCH routes are gone unless the SPA uses them (tests)
- [x] #3 Requests are logged by tower-http TraceLayer with the x-request-id kept (header behavior covered by a test); no param or body logging remains
- [x] #4 Imports, cards, stats, Discord drafts, card-recognition corrections, and webcam-table endpoints work from the SPA (vitest plus a portal check)
- [x] #5 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Typed inputs for the remaining handlers. Stats take QueryParams<DateRange> (stats::query::DateRange replaces the Value params across the stats module). Cards take CardSearch, PrintingsQuery, and ImageQuery. Card recognition takes CursorQuery and a JSON document body for corrections, which corrections::validate checks against Oracle's schema. Discord pending updates take PendingWinner (string snowflake). Imports take CsvUpload, JsonUpload, SheetRequest, and SheetCommit (flattened request plus revision); Choice deserializes only ids or new/skip/create, replacing validate_sheet. Paths are PathParam<i64|String>.
2. Delete web/params.rs (the Plug.Parsers emulation and its param logging) and changeset.rs; drop api::parse_id.
3. Router: method mismatches answer axum's 405 with Allow (no 404 fallback), the PUT aliases of PATCH routes are gone (PUT stays only on /api/admin/users/:id/player, which the SPA uses), and the docs drop the pipe_through framing.
4. Logging: request_id.rs keeps the x-request-id rule (a client id of 20 to 200 bytes is kept, otherwise generated, and the response echoes it). tower-http TraceLayer logs at info inside a span carrying method, path, and request_id, with status and latency, and never logs bodies or query strings.
5. Tests: 405 + Allow, the logging fields, typed query rejections, sheet/stats/import inputs through support::input. Docs: rust/README layout. Ran mise run precommit and a portal check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions: card-recognition corrections still take a free-form JSON document (JsonBody<Value>) because their schema belongs to Oracle and corrections.rs validates it field by field; it is a JSON body, not merged params. Method mismatches use axum default 405 (empty body, Allow header) rather than a JSON body. The SPA never triggers them. Unknown query keys such as cursor[]=1 are now ignored instead of being a 400, while wrongly typed known keys are still a 400. The debug-level param logging with Phoenix filter_parameters is gone: tower-http logs only method, path, status, latency, and request id, so there is nothing sensitive to filter. The sheet import revision fingerprint now hashes the typed request (sorted maps), so a preview taken before this release does not commit after it; previews are short-lived. The webcam socket handler keeps its Query extractor until TASK-1.8 replaces the socket. Lotus gaps: none.

Validation: mise run precommit exit 0 (Rust 56 unit and 610 integration tests, vp check, 481 vitest tests, build). Portal stack: Import > CSV preview of a 2-seat game ("1 game ready", 1 create), then Confirm import created it. The player stats time range switched to 1 month (GET /api/stats/players/1 with date filters returned 200). Card search /api/cards?q=sol&limit=3 returned Sol Grail, Sol Ring, Sol Talisman. PUT /api/players/1 answered 405 with Allow: GET,HEAD,PATCH,DELETE. The review-service log shows tower_http lines such as request{method=GET path="/api/stats/players/1" request_id="656dZyn3I-..."}: finished processing request latency=12 ms status=200.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Every remaining handler takes typed extractors (stats DateRange, card search, printings, image, corrections cursor, Discord winner, import uploads, typed SheetRequest with validated choices). web/params.rs and changeset.rs are deleted. Method mismatches answer 405 with Allow, the PUT aliases are gone, and request logging is tower-http TraceLayer with the x-request-id kept. Verified with mise run precommit and portal checks (CSV import preview and commit, stats range, card search, 405, log lines).
<!-- SECTION:FINAL_SUMMARY:END -->
