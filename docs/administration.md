# Administration

Accounts, sessions, audit history, the card catalog, and other server operations. Installing and
configuring the server is covered in [Installation](installation.md).

## Accounts and registration

The first account registered in the browser becomes the server administrator and is the only
password account. After bootstrap, members sign in with Discord. New Discord identities are
accepted only while **Open registration** is enabled under **Admin → Users**; existing linked
members can always sign in. This flag is stored in the singleton `server_settings` row.

For headless container bootstrap, set `THE_GATHERING_ADMIN_USERNAME` and
`THE_GATHERING_ADMIN_PASSWORD`. Startup creates the admin if absent and is idempotent on later
restarts. In a source checkout, the equivalent command is:

```sh
cd rust
THE_GATHERING_ENV=dev THE_GATHERING_ADMIN_PASSWORD='use-a-long-password' \
  mise exec -- cargo run -- create-admin USERNAME
```

**Detailed statistics from** on the same page sets a cutoff date for statistics that depend on
data your pod may not have recorded from the start: games before it still count toward wins and
losses, but their seat positions, game length, turn counts, and MVP cards are left out. See
[Statistics API](stats.md).

Administrators can disable and re-enable accounts, or permanently delete an account. Disabling an
account signs it out on every device; re-enabling it does not revive those sessions, so the user
must sign in again. Administrators can also sign an account out everywhere without changing whether
it is enabled. Deleting an account requires its linked player to have zero games and permanently
removes the account, sessions, player, and unused decks together. Games recorded by the account for
other players are preserved. Disable accounts with game history instead.

**Admin → Player identities** lists players (including archived players), their Discord IDs, and
linked accounts. Search by player name, Discord ID, or username, then unlink an incorrect identity
before merging players. Unlinking detaches both the Discord identity and account from the player,
preserving the player's games and decks. It does not delete the account or change its Discord login;
a future sign-in or import may create a separate player. Use **Admin → Users** to link the account
to the correct player. Identity management requires recent administrator authentication.

Sessions use random tokens stored in the `users_tokens` table, carried in an encrypted
`the_gathering_session` cookie. Upgrading from 0.2 or earlier keeps everyone signed in: the
first request with the old `_the_gathering_key` cookie moves its session into the new cookie.
Browser tabs left open across that upgrade need one reload before saving changes (their page
holds an old-format CSRF token). The session cookie and its token last 14 days and are reissued after 7, so members who
visit at least every two weeks stay signed in; Discord sign-in skips the consent screen once a
member has authorized the app. Signing out ends only the current device's session. Changing the
administrator password expires every session for that account, and expired session rows are pruned
when a new session is issued. Sensitive actions—including import commits and player merges—require
authentication within the previous ten minutes; previews and sample downloads remain available
without reauthentication.
The SPA prompts the administrator for a password and Discord members to authorize with Discord
again. Passwords must be 12–72 characters.

Password login and bootstrap registration (`POST /api/session`, `POST /api/users`) are limited
to 10 attempts per client address every 5 minutes, login additionally to 10 attempts per
username from any address, and both together to 100 attempts server-wide; further attempts get
`429 Too Many Requests` with a `retry-after` header. Behind a reverse proxy every request
arrives from the proxy's address, so set `TRUST_PROXY_HEADERS=true` when your proxy sets
`x-real-ip` or `x-forwarded-for`. The server believes those headers only when the request comes
from `TRUSTED_PROXIES` (loopback and private addresses unless set), so clients reaching port
4000 directly cannot pick their own address. Password reauthentication is separately limited by
account and client address, with a server-wide attempt budget. Discord sign-in is not rate
limited here.

Members can create personal API keys under **Settings → API keys** to list games from scripts
(`GET /api/v1/games`, filterable by player and date). A key acts as its owner with the same
permissions and stops working when the account is disabled. See [Personal API keys](api.md).

See [Discord integration](discord-integration.md) for bot creation, permissions, and current tracking behavior.
See [CSV game import](csv-import.md) for the spreadsheet format and admin import flow, and [Mythic Track import](mythic-track-import.md) for moving an existing Mythic Track playgroup over.

## Audit history and live logs

**Administration → Audit log** records member and administrator operations, with the actor's
identity at the time, target, request ID, response status, and paginated before/after snapshots.
Both audit history and **Server logs** require an administrator with authentication in the last
ten minutes. Search audit history by username, operation, or target, and filter by outcome.

History includes only operations with committed changes to the audited records below. HTTP API
mutations, Discord sign-in callbacks, and handled Discord commands/components carry attribution,
but completed operations without row snapshots are discarded. Routine sign-ins, webcam table
controls, deck-chooser calculations, no-op saves, and rejected requests without changes are not
listed; existing zero-change history is also hidden without deleting it. A sign-in that creates
or updates an account still appears. Discord operations are marked **Accepted**; inspect their
committed changes. HTTP failures with partial commits remain visible; an unknown outcome means
the request is still running or its final status was not saved.

SQLite triggers record safe columns of users, players, decks, games, game seats, server settings,
API-key metadata, sheet import receipts, pending Discord reports, and result drafts. Snapshots
commit and roll back with the data, including cascaded deletions. Actor attribution is scoped to
each write transaction; deleting or renaming an account does not erase its audit identity. Writes
outside a user operation (such as background jobs or SQL maintenance) retain snapshots with a
NULL `operation_id` in `audit_changes`, but are not listed as user operations in the panel.

History starts at this migration, is retained indefinitely in the application database, and has
no UI delete/clear action. For manual reconstruction, follow `audit_changes.id` order (requests
can overlap), using the related IDs in each snapshot. There is no automatic undo, complete initial
snapshot, or tamper-proof guarantee: database administrators can change this database. Keep database
backups. Audit history is not included in portable game exports. Transient webcam state, scheduled
Discord queues, card images/corrections, catalog/cache data, and external effects are not snapshotted.
Passwords, credential hashes, session tokens, invitation secrets, raw request bodies, and query
strings are excluded. Secret-only rotations that change no safe snapshot field are not retained
as audit operations; API-key creation/deletion and changes to whether a credential is set are.

**Server logs** follows the Rust process's tracing events over Server-Sent Events, using the same
`LOG_LEVEL`/`RUST_LOG` filter as stdout. It retains only the newest 200 messages in the browser,
does not replay old messages, and reports dropped messages if a reader falls behind. **Clear** only
clears that browser's buffer. Access is rechecked every five seconds and before each batch; logout,
revocation, demotion, or expired authentication ends the stream. Reverse proxies must permit
unbuffered, long-lived SSE responses. System service/container logs remain available separately.
ANSI/control characters and secret-named structured fields are removed, but free-form log messages
are not automatically redacted; application code must never log credentials.

## Card catalog

The app downloads Scryfall's compressed `default_cards` and `rulings` JSONL feeds when the catalog is empty or no printing has been imported yet (as after upgrading to a release that stores printings), and refreshes them weekly. The responses are streamed to temporary files and decoded incrementally, then a complete staged generation of cards and rulings is published atomically. Every printing in `default_cards` is stored too (printings a complete sync no longer lists are removed unless a deck uses them). Card search, printing pickers, printing details (including prices, as of the latest sync), and rulings use only SQLite after a sync; no page makes Scryfall API requests. Run a refresh manually from **Admin → Catalog → Sync now**, which shows the latest sync's state, or with:

```sh
docker exec the-gathering /app/bin/the-gathering catalog-sync
# from a checkout: (cd rust && THE_GATHERING_ENV=dev mise exec -- cargo run -- catalog-sync)
```

After each sync (and after every CSV or Mythic Track import) the app links decks and MVP cards that only carry a card name to catalog cards by name, filling in Scryfall IDs and missing colour identities. Trigger that alone from **Admin → Catalog → Link imported cards to the catalog** or with `the-gathering catalog-backfill`.

**Game Changer badges** use Scryfall's `game_changer` flag for the Commander Brackets list, not a separate hard-coded card list or a bracket recommendation. After deploying the Game Changer migration, run `the-gathering catalog-sync` once: existing catalog rows initially default to false, and linking/backfilling alone cannot populate this flag. Future catalog syncs keep the badges current, for printings as well as cards (browsers may cache printing details for one hour). Offline recognition-gallery candidates do not carry this metadata; their preview shows the badge once printing details load. No badge means no known positive flag, not a guarantee that the card is absent from the list. Deck summaries label each commander independently; the app does not store full decklists, so it cannot report a whole-deck Game Changer count.

There is one row per Scryfall `oracle_id`. The preferred printing is English, available on paper, non-digital, and non-promo, then the newest `released_at`; set code, collector number, and Scryfall UUID break ties. `default_cards` is used instead of `oracle_cards` because it provides printing images and lets the app choose that representative deterministically.

In **Deck details**, use **Choose printing** below the commander or partner to match the artwork on your card, then **Save deck**. **Use catalog default** removes the override. Printing choices come from the printings the catalog sync stores (English paper printings, newest first, with pagination), so browsing needs no Scryfall API request. Saved artwork survives catalog refreshes, even when Scryfall stops listing that printing. Image files use the shared cache described below. Changing a commander or partner clears that slot's printing; printing selection does not change commander identity, colors, imports, or statistics.

Card images (`small`, `normal`, `art_crop` JPEGs from `cards.scryfall.io`) are served through
authenticated `GET /api/card-images?url=…`. Stored catalog URLs remain unchanged; API responses
point browsers at this endpoint. It stores the original bytes under `DATA_DIR/card-images`
(`/data/card-images` in Docker), shares concurrent requests for the same URL, and fetches at
most four images concurrently. There are no new environment variables or services.

The disk cache is capped at **512 MiB**, evicting oldest-written files first on startup and
insertion; files expire after 30 days. Budget up to an additional 2 MiB for an atomic write.
It is disposable and can be omitted from backups or cleared while the app is stopped. Browsers
cache for one day (`private, max-age=86400`, content ETag). URL timestamps remain part of the
cache key so replacement scans do not collide. Cache misses require internet access; hits do
not. Disk-write failures still serve the downloaded image without caching it.

Only the exact Scryfall image origin and recognized image paths are accepted; redirects are
not followed, downloads over 2 MiB or non-JPEG responses are rejected, and only signed-in users
can fetch. Upstream errors are not cached or automatically retried; HTTP 429 pauses new misses
for at least 30 seconds (or the numeric `Retry-After`, if longer). At most 128 distinct misses
can wait or run; excess requests return 502. Scryfall's current [rate limits](https://scryfall.com/docs/api/rate-limits)
exempt image file origins; the separate card-data API limits remain in force. Images are not
resized, transformed, or stripped of artist/copyright information.

A card can be a commander when, per Comprehensive Rules 903.3, its front face is a legendary creature, Vehicle, or Spacecraft, or its oracle text says it can be your commander. Backgrounds are deliberately excluded. Partner, Partner with, Friends forever, Choose a Background, and Background are stored as a separate pairing classification for deck-building interfaces.

