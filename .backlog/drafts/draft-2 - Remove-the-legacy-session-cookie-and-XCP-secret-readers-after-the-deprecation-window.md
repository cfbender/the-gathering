---
id: DRAFT-2
title: >-
  Remove the legacy session cookie and XCP secret readers after the deprecation
  window
status: Draft
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:49'
labels: []
dependencies: []
type: chore
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-1.1 keeps read-only code for the Plug-format `_the_gathering_key` session cookie (upgraded to the native cookie on first request) and for `XCP.` encrypted ManaVault API keys (re-encrypted at boot). That code and the `eetf`/`pbkdf2` dependencies can go once every install has run a release containing TASK-1.1 and live sessions have rolled over (14-day session validity). TASK-1.2 likewise accepts `SECRET_KEY_BASE`/`PHX_*` with a warning. Do this in a release at least 14 days and one tagged release after TASK-1.1 and TASK-1.2 shipped, and note the minimum upgrade path in the changelog.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The legacy cookie reader, XCP decryption, the eetf dependency, and pbkdf2 (if otherwise unused) are removed
- [ ] #2 Release notes state that installs must upgrade through a release containing TASK-1.1 first
- [ ] #3 mise run precommit passes
<!-- AC:END -->
