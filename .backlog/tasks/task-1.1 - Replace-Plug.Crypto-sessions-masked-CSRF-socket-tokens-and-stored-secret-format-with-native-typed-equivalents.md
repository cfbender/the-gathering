---
id: TASK-1.1
title: >-
  Replace Plug.Crypto sessions, masked CSRF, socket tokens, and stored-secret
  format with native typed equivalents
status: Done
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:48'
updated_date: '2026-10-07 22:11'
labels: []
dependencies: []
parent_task_id: TASK-1
priority: medium
type: enhancement
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`crypto.rs` reimplements Plug.Crypto (PBKDF2 KeyGenerator, MessageVerifier, Erlang external term format via `eetf`, masked CSRF tokens). `web/session.rs` stores a string-keyed map of `eetf::Term`s in the `_the_gathering_key` cookie, and `web/api/accounts.rs` builds an Ueberauth-shaped term map for the Discord OAuth attempt. The cookie needed to match the Elixir release, and the Elixir release no longer exists. Unlike ManaVault, live sessions must survive the upgrade: the cookie only carries a `users_tokens` token, so the old cookie can be read once and reissued. Socket tokens and stored ManaVault API keys also use `Plug.Crypto.encrypt`. Stored keys must keep decrypting; nothing else needs Plug compatibility. Keep the `users_tokens` design (validity, 7-day reissue, logout and admin revocation disconnecting sockets), the return-to flow, the invite-hash flow, sudo, and bcrypt.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Sessions are a typed struct in an axum-extra private cookie; Plug MessageVerifier/KeyGenerator signing, CSRF masking, and string-keyed eetf session maps are gone from the request path
- [x] #2 A browser holding a valid legacy _the_gathering_key cookie stays signed in after the upgrade: it gets the new cookie and the legacy cookie is expired (integration test)
- [x] #3 Login, logout, 7-day token reissue, admin session revocation (including socket disconnect), return-to, registration invite, Discord OAuth state and sudo keep working, covered by tests
- [x] #4 State-changing /api requests without a valid x-csrf-token get the JSON 403
- [x] #5 Webcam table socket tokens use a native format with the same one-day expiry; invalid, expired, or revoked tokens are refused with 403
- [x] #6 Stored ManaVault API keys written by earlier releases still decrypt (fixture test) and are re-encrypted to the native format at boot; new writes use the native format
- [x] #7 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add axum-extra (cookie-private). web/session.rs: typed SessionData { csrf_token, user_token, registration_invite_hash, discord_oauth: DiscordOAuthAttempt { state, return_to, sudo_discord_id, registration_invite_hash } } as JSON in the PrivateCookieJar cookie the_gathering_session (14 days, HttpOnly, SameSite=Lax, Path=/); Key = SHA-512("the-gathering.session.v1\0" || secret), held in AppState. The Session request handle keeps the write-back pattern with typed update/renew/csrf_token methods. Drop live_socket_id and user_return_to: neither was ever read.
2. legacy.rs (read-only): verify the _the_gathering_key cookie (PBKDF2 key, salt sQwWhYdP, SFMyNTY. HMAC, ETF map) and carry user_token into a new session; every request carrying the legacy cookie gets a Max-Age=0 removal. Also decrypts XCP. credentials.
3. CSRF: 32 random bytes (base64url) in SessionData, compared in constant time with x-csrf-token, with no masking.
4. Socket tokens: crypto::seal (XChaCha20-Poly1305, purpose-derived key) over JSON {session, expires_at}, valid for one day.
5. Stored secrets: enc.v1.<sealed>. Reads accept enc.v1. and legacy XCP.; Accounts::reencrypt_legacy_secrets runs at boot after migrations.
6. crypto.rs: native primitives only (random, secure_compare, sha256, base64, seal/open).
7. Tests: harness decrypts and encrypts the cookie, plus legacy cookie upgrade (Elixir-signed vector), expired legacy token, legacy credential decrypt and re-encrypt, expired and tampered socket tokens, and the session unit tests.
8. Docs: README sessions and upgrade note, rust/README layout, docs/webcam-table socket token.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions: cookie the_gathering_session holds JSON SessionData encrypted by axum-extra PrivateCookieJar (AES-GCM, key = SHA-512 of a purpose label and the secret). Legacy _the_gathering_key cookies are upgraded on first request (user_token carried over, legacy cookie expired); the rest of the old session is dropped. user_return_to and live_socket_id were written but never read, so they are gone. The SPA return path is client-side and the OAuth returnTo lives in DiscordOAuthAttempt. Socket tokens and stored credentials share crypto::seal with distinct purpose labels. Stored credentials are now enc.v1.; XCP. values still decrypt and are rewritten by Accounts::reencrypt_legacy_secrets at every boot (a no-op once converted). eetf, pbkdf2, and hmac are now used only in legacy.rs (DRAFT-2 removes them). Lotus gaps: none.

Validation: mise run precommit exit 0 (fmt, clippy -D warnings, 50 unit + 606 integration tests, vp check, vitest, build). The untracked Elixir-era deps/ and _build/ directories in the orb broke vp fmt; they were moved to /home/user/workspace/stale-elixir-build, not deleted. Live server (review service): a legacy cookie signed with the dev secret for a legacy-member token returned that member from /api/session, set the_gathering_session, and expired _the_gathering_key, and the new cookie alone stayed signed in. PATCH /api/session/appearance returned 403 without or with a wrong x-csrf-token and 200 with the shell meta token. In the browser (portal stack), the Settings palette changed to nord and saving a ManaVault key stored enc.v1.…; /table/<uuid> connected the socket and seated the player (artifacts task-1.1-*.png).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the Plug.Crypto emulation with native typed equivalents. Sessions are a typed SessionData in an axum-extra private cookie (the_gathering_session), CSRF tokens are plain constant-time-compared tokens, and socket tokens and stored credentials use XChaCha20-Poly1305 seal/open with purpose keys. A read-only legacy module keeps upgrades seamless: old session cookies are converted on first request, and XCP. ManaVault keys decrypt and are re-encrypted at boot. Verified with mise run precommit, new integration tests (Elixir-signed cookie and credential vectors), and live checks on the review service and portal stack.
<!-- SECTION:FINAL_SUMMARY:END -->
