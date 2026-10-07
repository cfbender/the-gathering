---
id: TASK-1.6
title: >-
  Typed extractors for the remaining endpoints, remove Params/Changeset, and
  native router/logging behavior
status: To Do
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:49'
updated_date: '2026-10-07 21:49'
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
- [ ] #1 web/params.rs and changeset.rs are deleted; no handler takes merged query-and-body params
- [ ] #2 API method mismatches answer 405 with an Allow header; PUT aliases of PATCH routes are gone unless the SPA uses them (tests)
- [ ] #3 Requests are logged by tower-http TraceLayer with the x-request-id kept (header behavior covered by a test); no param or body logging remains
- [ ] #4 Imports, cards, stats, Discord drafts, card-recognition corrections, and webcam-table endpoints work from the SPA (vitest plus a portal check)
- [ ] #5 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Typed inputs for `web/api/{imports,cards,stats,discord,cardid,webcam}.rs`. Imports keep their multipart and large-JSON uploads within the 8 MB limit; the limit stays but is documented as ours. Stats filters become `Query<StatsFilters>`. Discord pending updates and result drafts get typed bodies. Portable import row validation uses `Validator` instead of `Changeset::new` over a `Value`.
2. Delete `web/params.rs` and `changeset.rs` (the remaining helpers move to their users or to `validation.rs`).
3. Router: let axum answer 405 with `Allow` for method mismatches on API routes (the SPA catch-all stays GET). Mount only the verb the SPA uses (PATCH, or PUT for admin link player) and remove the PUT aliases. Rewrite the `web/mod.rs` docs without `pipe_through` framing.
4. Logging: replace `request_id.rs`'s Phoenix-style lines with tower-http `TraceLayer` plus `SetRequestId`/`PropagateRequestId` (`x-request-id`, accepting a client id of 20 to 200 bytes as before) and structured fields (method, path without query, status, latency). Drop body and param logging; nothing sensitive is logged.
5. Update the frontend callers whose shapes change and their tests. Update the integration tests (imports_api, card_api, cardid_api, discord_api, stats_api, webcam_api, web_foundation). Run `mise run precommit`. In the portal, run a CSV import preview and commit, a card search, a stats page with filters, and a webcam-table room list, and confirm request ids in the service log.
<!-- SECTION:PLAN:END -->
