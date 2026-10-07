---
id: TASK-1.1
title: >-
  Replace Plug.Crypto sessions, masked CSRF, socket tokens, and stored-secret
  format with native typed equivalents
status: To Do
assignee:
  - '@cfbender'
created_date: '2026-10-07 21:48'
updated_date: '2026-10-07 21:48'
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
- [ ] #1 Sessions are a typed struct in an axum-extra private cookie; Plug MessageVerifier/KeyGenerator signing, CSRF masking, and string-keyed eetf session maps are gone from the request path
- [ ] #2 A browser holding a valid legacy _the_gathering_key cookie stays signed in after the upgrade: it gets the new cookie and the legacy cookie is expired (integration test)
- [ ] #3 Login, logout, 7-day token reissue, admin session revocation (including socket disconnect), return-to, registration invite, Discord OAuth state and sudo keep working, covered by tests
- [ ] #4 State-changing /api requests without a valid x-csrf-token get the JSON 403
- [ ] #5 Webcam table socket tokens use a native format with the same one-day expiry; invalid, expired, or revoked tokens are refused with 403
- [ ] #6 Stored ManaVault API keys written by earlier releases still decrypt (fixture test) and are re-encrypted to the native format at boot; new writes use the native format
- [ ] #7 mise run precommit passes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add `axum-extra` (cookie-private). `web/session.rs` becomes a typed `SessionData { csrf_token, user_token: Option<Token>, return_to, registration_invite_hash, discord_oauth: Option<DiscordOAuthAttempt { state, return_to, sudo_discord_id, registration_invite_hash }> }` serialized as JSON into a `PrivateCookieJar` cookie `the_gathering_session` (14-day max-age, HttpOnly, SameSite=Lax). The `Key` is derived with SHA-512 over a domain-separated prefix and the secret. Keep the request-extension handle and write-back middleware pattern: handlers mutate typed fields instead of string keys. Drop `live_socket_id`.
2. Add a legacy upgrade module (`web/session/legacy.rs`, the only `eetf` user besides stored-secret decryption): when the new cookie is absent and `_the_gathering_key` verifies (PBKDF2 key, salt `sQwWhYdP`, `SFMyNTY.` HMAC), take `user_token` (and `user_return_to`), start a typed session, set the new cookie, and expire the legacy cookie. Everything else in the old session is dropped. Its removal is tracked in a deferred draft.
3. CSRF: the token is 32 random bytes (base64url) in `SessionData`, compared in constant time with `x-csrf-token`, with no masking. The SPA shell meta tag and the post-login and post-logout `x-csrf-token` header keep working as is. Tabs left open across the upgrade get a 403 on their next mutation and need a reload; this is noted in the changelog.
4. Socket tokens: XChaCha20-Poly1305 over `{session_token, expires_at}` serialized as JSON (or raw bytes plus an expiry) with a key derived from the secret. Same one-day expiry, same `token` connect param. The client already refetches the config token when the socket errors.
5. Stored secrets: write `users.manavault_api_key` as `enc.v1.<base64url(nonce || ciphertext)>` (XChaCha20-Poly1305 with a SHA-256-derived key). Reads accept `enc.v1.` and legacy `XCP.` values. At boot, after migrations, re-encrypt every `XCP.` value in one transaction, logging the count. Keep a fixture test with an Elixir-written `XCP.` value.
6. Shrink `crypto.rs` to native primitives (random bytes, constant-time compare, sha256, base64 helpers, `seal`/`open` for secrets and socket tokens). Move Plug-format code (PBKDF2 KeyGenerator, MessageVerifier verify, `XCP.` decrypt, ETF decode) into the legacy module, read-only. The `pbkdf2` crate stays only for legacy reads.
7. Rewrite the test harness (`tests/integration/support`) for the new cookie. Add tests: cookie round trip and tampering; legacy cookie upgrade keeps the user signed in and expires the old cookie; CSRF 403 when missing or wrong; Discord OAuth state round trip; socket token expiry and revocation; legacy `XCP.` decryption and boot re-encryption.
8. Update docs (rust/README layout, README security notes) and run `mise run precommit`. In the portal, check login, logout, the Discord-less sudo flow, a webcam-table socket connect, and a session created with the old cookie format surviving the switch.
<!-- SECTION:PLAN:END -->
