<img src="priv/static/images/logo.svg" alt="" width="96" align="right" />

# The Gathering

A self-hosted tracker for Commander (Magic: The Gathering) games. Record who played, which commanders, who won, and how, then browse stats for your playgroup. One container, one SQLite file, no external services.

**Status:** early scaffold. The stack, container build, and developer setup are in place; game tracking features are next.

## Planned features

- Discord sign-in for members; the server administrator uses username/password.
- Log games by hand or import them from CSV.
- Automatic game tracking from SpellBot / Discord.
- Card search backed by a Scryfall catalog (latest printing per card) for commanders and MVP cards.
- Deck-list links for Moxfield, Archidekt, and manavault.
- Playgroup, player, commander, and deck statistics.

## Self-hosting

Requirements: Docker with Compose.

1. Create a directory and download [`docker-compose.yml`](docker-compose.yml) and [`.env.example`](.env.example) into it.
2. Copy `.env.example` to `.env` and set `THE_GATHERING_SECRET_KEY`:

   ```sh
   openssl rand -base64 64 | tr -d '\n'
   ```

   Set `THE_GATHERING_PUBLIC_URL` to the address users reach the app at (for example `https://games.example.com` behind a reverse proxy).
3. Start it:

   ```sh
   docker compose up -d
   ```

The app listens on port 4000 and stores its SQLite database and files under `./data` (mounted at `/data`). Back up that directory to back up everything. TLS is left to your reverse proxy.

Health check: `GET /api/health` returns `{"status":"ok"}` when the database is reachable.

### Updating from the admin UI

**Administration → Server settings → Software update** shows the running version and whether
GitHub has a newer one (the latest release for `vX.Y.Z` builds, the newest `main` commit for
`nightly-<commit>` builds, the `preview` tag's commit for `preview-<commit>` builds; the check is
cached for 15 minutes). The **Update now** button installs
the newest build of the channel the server already follows; it does not switch channels. The app
never replaces itself, it asks whatever runs it:

- Proxmox LXC: the installer (and every `update` run) sets up a `the-gathering-update.path`
  systemd unit that runs `update` as root when the app writes
  `/var/lib/the-gathering/update-request`. Containers created before this existed get it the next
  time `update` runs.
- Docker: an optional [Watchtower](https://watchtower.nickfedor.com) sidecar. In `.env` set
  `COMPOSE_PROFILES=self-update` and `WATCHTOWER_HTTP_API_TOKEN` (`openssl rand -hex 32`), then
  `docker compose up -d`. The button then asks Watchtower to pull the image tag the container was
  started from (`latest` follows `main`; pin `0.2` or a `v0.2.0` tag for releases) and recreate
  the container. Watchtower does not poll on its own in this setup.

When neither is configured the section only shows the version and says the server has to be
updated by hand. The version comes from `priv/VERSION`, which the release and container builds
write; development builds have none.

### Proxmox VE

[`deploy/proxmox/the-gathering.sh`](deploy/proxmox/the-gathering.sh) creates an unprivileged
Debian LXC that runs the app natively (no Docker): it installs the release tarball (the Rust server and the built web app) from the
latest [GitHub release](https://github.com/cfbender/the-gathering/releases) into
`/opt/the-gathering`, keeps data in `/var/lib/the-gathering`, reads settings from
`/etc/the-gathering.env` (same keys as `.env.example`), and runs it as the `the-gathering`
systemd service. Run it as root on the Proxmox host. It asks for the public URL and the first
administrator account; container settings such as `CTID`, `STORAGE`, `IP`, `CORES`, `RAM_MB` and
`VERSION` are environment variables documented at the top of the script. By default it takes the
next free container id, lets DHCP assign an address and then stores that address as the
container's static IP (`PIN_IP=false` keeps DHCP), and uses the Proxmox host's time zone
(`TIMEZONE`).

```sh
bash -c "$(curl -fsSL https://raw.githubusercontent.com/cfbender/the-gathering/main/deploy/proxmox/the-gathering.sh)"
```

Updates work like the community-scripts helpers: inside the container, `update [tag]` is on `PATH`
and installs the latest (or given) release and restarts the service (it runs the current copy of
the script from GitHub, so fixes to the updater reach existing containers). From the Proxmox host,
`bash the-gathering.sh update <CTID> [tag]` does the same, as does **Update now** in the admin UI
(see above; it logs to `journalctl -t the-gathering-update`). The previous release stays under
`/opt/the-gathering/releases/` for rollback. Release tarballs are built by
[`.github/workflows/release.yml`](.github/workflows/release.yml) for every `v*.*.*` tag, and every
push to `main` republishes the rolling
[`nightly`](https://github.com/cfbender/the-gathering/releases/tag/nightly) prerelease. `update
nightly` (or `VERSION=nightly` at install time) switches a container to that channel: from then on
untagged and automatic updates follow `main`, and `update vX.Y.Z` returns to tagged releases.

To try a branch before it is merged, run the **Release** workflow manually on that branch
(Actions → Release → Run workflow, or `gh workflow run release.yml --ref <branch>`). It republishes
the rolling [`preview`](https://github.com/cfbender/the-gathering/releases/tag/preview) prerelease
from that branch (version `preview-<commit>`) and never touches `nightly` or the latest release.
Install it with the branch's copy of the script, since `main`'s may not know the channel yet:

```sh
bash -c "$(curl -fsSL https://raw.githubusercontent.com/cfbender/the-gathering/<branch>/deploy/proxmox/the-gathering.sh)" \
  the-gathering.sh update <CTID> preview
```

From then on untagged and automatic updates (and **Update now**) follow `preview`, using the
script at the `preview` tag; `update vX.Y.Z` or `update nightly` leaves the channel.

The container also updates itself on a cron schedule: `AUTO_UPDATE` (asked at install time;
default `0 4 * * *`, daily at 04:00 in the container's time zone; `off` disables it) is written to
`/etc/cron.d/the-gathering-update`, and each run logs to `journalctl -t the-gathering-update`.
Change it later with `bash the-gathering.sh auto-update <CTID> '<cron expression>'` or
`... auto-update <CTID> off` (without the `<CTID>` when run inside the container). Containers
created before the time zone option existed run on UTC; `pct set <CTID> --timezone host` followed
by a restart moves them to the host's zone.

### Invite members while registration is closed

Under **Administration → Server settings → Sign-up invitation**, create a reusable
Discord sign-up link. Copy and save it before leaving the page: only its hash is
stored, so the secret cannot be retrieved later. Anyone with the link can join as
a member even with **Open registration** off. Links do not expire or limit uses.

**Rotate link** generates a replacement and immediately revokes the old link's
registration permission, including OAuth sign-ups that have not completed yet.
Already registered users keep their access; disabled accounts remain blocked.
Normal open registration and the initial password administrator setup are unchanged.
Creating or rotating links requires administrator sudo authentication. The secret
uses a URL fragment (not a logged request path or query), then a signed session
carries its digest through Discord OAuth. Treat the full link as a credential.

### Deck-list links

`POST /api/decklists/resolve` accepts `{"url":"..."}` for a public Moxfield,
Archidekt, or self-hosted ManaVault deck (when `MANAVAULT_URL` is set). Successful responses contain the canonical
URL, deck name, commanders, commander color identity, author when exposed, card
count, and fetch timestamp under `data`. Successful lookups are cached in memory
for five minutes; errors are never cached.

`GET /api/decks/:id/decklist` returns the playable card list behind a deck's linked
Moxfield, Archidekt, or ManaVault page: each card's name, quantity, zone
(`commander` or `mainboard`; maybe-, side- and ManaVault "considering" boards are left
out), the exact Scryfall printing when the list records one, and type, mana cost and
cached images from the local catalog. It reuses the same live fetch and five-minute
cache; decks without a supported link, and missing or private lists, return 404. The
app keeps no copy of the list.

Each user can also save a Moxfield username, Archidekt username, and ManaVault
instance URL plus personal API key under **Settings → Profile → Deck hosts**.
`PATCH /api/session/user` accepts these as `moxfield_username`, `archidekt_username`,
`manavault_url`, and `manavault_api_key`. The API key is write-only: it is stored
encrypted with `THE_GATHERING_SECRET_KEY` (keys stored by 0.2 and earlier are re-encrypted in the
current format at startup), never returned (user JSON exposes only
`has_manavault_api_key`), a blank value keeps the saved key, and `null` removes it.
`GET /api/session/remote-decks` returns the signed-in user's normalized public decks
(`name`, commanders, color identity, URL, source, and upstream update time) plus a
status for each source. Results, including source errors, are cached in memory for
five minutes. The user's player page links these decks, and the new-game form can
resolve one into a new deck with a quick pick.

### Appearance

**Settings → Appearance** picks a color palette (Claret, Nord, Catppuccin, Tokyo Night,
Gruvbox, Everforest, Kanagawa, Night Owl, Dracula, Rosé Pine, Solarized, or Monochrome) and a surface style (Liquid glass or
Classic). Both are saved on the user account through `PATCH /api/session/appearance`
(`{"palette": "...", "theme_style": "glass" | "classic"}`), returned in user
JSON, and rendered onto `<html>` by the SPA shell so the first paint already matches on
any device. Light/dark mode stays a per-device preference on the navigation toggle.

The integrations use the upstream services' public interfaces:

- Moxfield: `GET https://api2.moxfield.com/v3/decks/all/:id`. The request sends a
  descriptive User-Agent, but Moxfield does not publish API limits or a supported
  third-party API contract and may reject server traffic with Cloudflare 403s. User
  listings use the unofficial `GET https://api2.moxfield.com/v2/decks/search-sfw`
  endpoint and report upstream blocking without failing the rest of the list.
- Archidekt: `GET https://archidekt.com/api/decks/:id/`. No authentication or
  documented public rate limit is currently required. User listings use the
  unofficial `GET https://archidekt.com/api/decks/v3/?ownerUsername=...` endpoint,
  then fetch public deck details to identify commanders.
- ManaVault: `POST $MANAVAULT_URL/share/graphql` against the ManaVault instance you
  configure. No authentication is required; ManaVault's default limit is 120
  requests per IP per minute. Author is not exposed by its public schema. Only the
  configured origin is recognized; other origins are intentionally not fetched to
  avoid SSRF. Without `MANAVAULT_URL`, ManaVault links are stored as plain deck links.
  Listing a user's ManaVault decks uses their own instance URL and personal API key
  (`GET <instance>/api/v1/decks` with `Authorization: Bearer <key>`, paginated with
  `page`/`per_page`); the URL must be an HTTPS origin with no path, credentials,
  query, or fragment. Private, loopback, link-local, and Tailscale destinations are
  blocked unless the exact hostname is listed in `MANAVAULT_ALLOWED_HOSTS`; those
  explicitly trusted hosts may also use HTTP. `MANAVAULT_ALLOW_INSECURE_URLS=true`
  permits HTTP for otherwise-public hosts without permitting private destinations.
  Redirects are not followed. The key is created under ManaVault's **Settings →
  Personal API keys**. Listings stop after 20 pages, 500 decks, 2 MB of responses,
  or 15 seconds per source and report truncation in that source's status. Because
  an authenticated listing may include unshared decks,
  quick picks from it prefill the deck form directly rather than re-resolving a public
  share link. Without a key, the source reports that one is needed and individual
  public share links continue to resolve normally.

### Deck chooser

**Decks → Choose a deck** picks from the signed-in user's linked player's active
decks. Selection uses ManaVault's weighting: hours since last played, multiplied by
current skips plus one, divided by recorded plays plus one. Never-played decks get a
30-day boost beyond the stalest played deck. Plays and last-played dates come from
game seats; only the current skip count and per-deck chooser inclusion setting are
stored. Skipping records the skip before rerolling, while choosing clears that deck's
skips.

Hosted decks are not listed separately: **Sync hosted decks** (on your player profile
and the chooser, shown once any deck host is configured) calls `POST
/api/session/remote-decks/sync`, which folds every listed Moxfield, Archidekt, and
ManaVault deck into your player's deck list. A remote deck refreshes the local deck
with the same deck-list URL or name; otherwise it links the unlinked local deck with
the same commander pair (filling in commander card, partner, and colors but keeping
your name for it); otherwise it is added as a new deck. A deck that already links
elsewhere is never re-pointed. Hosts that fail to list are skipped and reported in the
response's `errors`.

### Webcam table

**Games → Play** opens a temporary room for up to ten signed-in members who share their
board cameras through a WebRTC SFU built into the server (each browser uploads one simulcast
stream and receives every other board at the resolution it is drawn) and record the result
when the game ends. The SFU listens on the UDP range `WEBRTC_SFU_PORT_RANGE`; forward it to the
host and set `WEBRTC_SFU_PUBLIC_IP`, or set `WEBRTC_SFU_RELAY_ONLY=true` to carry media over the
configured TURN servers instead (see `docs/webcam-table.md`).
**Reveal hand to** privately sends your camera to one chosen player; everyone else receives no
video until you end the reveal or the target leaves. Clicking a card on
any board identifies it with the recognizer bundle published from [Oracle](https://github.com/cfbender/oracle) (see
its README, "Shipping"): The server serves `DATA_DIR/cardid/current/*` at `/api/cardid/*`
and the browser runs the models itself. A recognized card opens with its rules text (fetched
from Scryfall per printing and cached) and lands in that board's card tray for every seat.
Without a published bundle the table still works and offers the player's commanders as
suggestions instead. When a seated player's chosen deck links to a list, every seat loads it
and warms its card images, **Decks → View decklist** shows your own list, and the recognizer
favours cards in the clicked board owner's list (see Oracle's README, "Deck-list prior").
Opponents' lists are not shown in the UI but are visible in the browser's network tab.
**Trackers** over your own camera keep free-form 0–100 counters ("Lands", "Creatures in graveyard")
and a combat-math list: enter each anthem or combat buff with its conditions (attacking, blocking,
flying, tokens, nontoken, or any creature type) and the overlay shows the combined bonus for every
kind of creature ("Attacking token creatures +4/+4 · vigilance"). Each counter and the buff list has
its own "Show to table" switch; shared ones appear on your seat for everyone and shared counter
changes are logged, while private ones stay in your browser.
Design notes are in
[docs/webcam-table.md](docs/webcam-table.md); to improve recognition or contribute training data,
see Oracle's [CONTRIBUTING.md](https://github.com/cfbender/oracle/blob/main/CONTRIBUTING.md).

### Environment variables

| Variable | Default | Purpose |
| --- | --- | --- |
| `THE_GATHERING_SECRET_KEY` | required | Encrypts the session cookie, socket tokens, and stored ManaVault API keys. At least 64 bytes; keep it when upgrading, or stored keys stop decrypting. Releases up to 0.2 called it `SECRET_KEY_BASE`, which still works and logs a warning. |
| `DATA_DIR` | `/data` | Where the database, uploaded files, and the `cardid/` recognizer bundles live. |
| `DATABASE_PATH` | `$DATA_DIR/the_gathering.db` | SQLite database file. |
| `THE_GATHERING_PUBLIC_URL` | `https://localhost` | Origin users reach the app at (scheme, host, and a non-default port), used for generated links and the Discord OAuth redirect. Replaces `PHX_SCHEME`, `PHX_HOST`, and `PHX_URL_PORT`, which still work and log a warning. |
| `PORT` | `4000` | Port the server binds inside the container. |
| `TRUST_PROXY_HEADERS` | unset | Set to `true` behind a reverse proxy so rate limiting identifies clients by `x-real-ip` / `x-forwarded-for` instead of the proxy address. |
| `LOG_LEVEL` | `info` | `debug`, `info`, `warning`, or `error`. `debug` explains why the Discord bot ignored a message. |
| `CATALOG_SYNC_INTERVAL_HOURS` | `168` | Hours between automatic Scryfall catalog refreshes. |
| `WEBRTC_SFU_PORT_RANGE` | `50000-50100` | UDP ports the built-in webcam-table SFU listens on, one per connected browser. Forward the same range from your router to the host (Docker: publish it as `/udp`). |
| `WEBRTC_SFU_PUBLIC_IP` | unset | Public address browsers reach the SFU ports at, announced as an ICE candidate. Required for players outside your LAN unless `WEBRTC_SFU_RELAY_ONLY` is set. |
| `WEBRTC_SFU_IPV6` | unset | Set to `true` to also offer the host's IPv6 addresses as media candidates. Off by default: the port forward above is IPv4, and LAN browsers that pick an IPv6 path have lost their video on it. |
| `WEBRTC_SFU_RELAY_ONLY` | unset | Set to `true` to open no public ports: the SFU reaches browsers through Cloudflare TURN (requires `CLOUDFLARE_TURN_KEY_ID`), so all media crosses the relay. |
| `WEBRTC_STUN_URLS` | Google + Cloudflare public STUN | Comma-separated STUN URLs browsers use for NAT discovery toward the SFU. `none` disables STUN for LAN-only installs. |
| `WEBRTC_TURN_URLS` | unset | Comma-separated TURN URLs; with `WEBRTC_TURN_USERNAME` and `WEBRTC_TURN_CREDENTIAL`, relays webcam-table media for browsers that cannot reach the SFU directly. |
| `CLOUDFLARE_TURN_KEY_ID` | unset | With `CLOUDFLARE_TURN_API_TOKEN`, a Cloudflare Realtime TURN key. The server mints per-join credentials that expire after `CLOUDFLARE_TURN_TTL_SECONDS` (default 21600); relayed traffic is free up to 1,000 GB/month, then $0.05/GB. |
| `MANAVAULT_URL` | unset | Origin of a self-hosted ManaVault instance whose shared deck links are recognized and resolved. |
| `MANAVAULT_ALLOWED_HOSTS` | unset | Comma-separated exact hostnames allowed for personal ManaVault listing on private networks; also permits HTTP for those hosts. |
| `MANAVAULT_ALLOW_INSECURE_URLS` | unset | Set to `true` to permit HTTP personal ManaVault origins that resolve to public addresses. |
| `DISCORD_CLIENT_ID` | unset | Discord application client ID; enables member OAuth sign-in when paired with the secret. |
| `DISCORD_CLIENT_SECRET` | unset | Discord application client secret. |
| `DISCORD_BOT_TOKEN` | unset | Enables the optional Discord bot: SpellBot tracking, `/log`, `/summary`, and `/newgame` webcam-table queues. |
| `DISCORD_GUILD_ID` | unset | Optional server ID for immediate guild-scoped `/log`, `/summary`, and `/newgame` registration; without it commands are global. Restricts invocation to that server when set. |
| `DISCORD_DEFAULT_TIMEZONE` | `America/New_York` | IANA timezone for `/newgame start:8pm` and `tomorrow 7pm`. Discord displays parsed timestamps in each viewer's local time. |
| `DISCORD_SPELLBOT_USER_ID` | `725510263251402832` | Discord user ID accepted as SpellBot, useful when running a private SpellBot deployment. |
| `THE_GATHERING_ADMIN_USERNAME` | unset | Creates this admin on container startup when paired with the password. |
| `THE_GATHERING_ADMIN_PASSWORD` | unset | Password for container or Mix-task admin bootstrap. |

Use `/newgame [start] [min_players] [title] [format]` to post a public Join/Maybe/Leave
queue. The default minimum is three (configurable from two to ten). Without
`start`, it opens a webcam-table link as soon as the minimum joins; scheduled
queues start or expire at the requested time. Maybe doesn't count toward the
minimum, but if a scheduled game is short at its start time the bot pings the
maybe list and waits 15 minutes for them to join. The host or a Discord Administrator
can Change time or Cancel. See [Discord integration](docs/discord-integration.md#webcam-table-queues-with-newgame)
for time syntax, permissions, and restart/delivery behavior.

### Accounts and registration

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
[Statistics API](docs/stats.md).

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
to 10 attempts per client address every 5 minutes; further attempts get `429 Too Many Requests`
with a `retry-after` header. Behind a reverse proxy every request arrives from the proxy's
address, so set `TRUST_PROXY_HEADERS=true` when your proxy sets `x-real-ip` or
`x-forwarded-for`. Password reauthentication is separately limited by account and client address,
with a server-wide attempt budget. Discord sign-in is not rate limited here.

Members can create personal API keys under **Settings → API keys** to list games from scripts
(`GET /api/v1/games`, filterable by player and date). A key acts as its owner with the same
permissions and stops working when the account is disabled. See [Personal API keys](docs/api.md).

See [Discord integration](docs/discord-integration.md) for bot creation, permissions, and current tracking behavior.
See [CSV game import](docs/csv-import.md) for the spreadsheet format and admin import flow, and [Mythic Track import](docs/mythic-track-import.md) for moving an existing Mythic Track playgroup over.

### Audit history and live logs

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

### Card catalog

The app downloads Scryfall's compressed `default_cards` JSONL feed when the catalog is empty and refreshes it weekly. The response is streamed to a temporary file and decoded incrementally, then a complete staged generation is published atomically. Search uses only SQLite after sync; run a refresh manually with:

```sh
docker exec the-gathering /app/bin/the-gathering catalog-sync
# from a checkout: (cd rust && THE_GATHERING_ENV=dev mise exec -- cargo run -- catalog-sync)
```

After each sync (and after every CSV or Mythic Track import) the app links decks and MVP cards that only carry a card name to catalog cards by name, filling in Scryfall IDs and missing colour identities. Trigger that alone from **Admin → Users → Link imported cards to the catalog** or with `the-gathering catalog-backfill`.

**Game Changer badges** use Scryfall's `game_changer` flag for the Commander Brackets list, not a separate hard-coded card list or a bracket recommendation. After deploying the Game Changer migration, run `the-gathering catalog-sync` once: existing catalog rows initially default to false, and linking/backfilling alone cannot populate this flag. Future catalog syncs keep the badges current. Printing searches and webcam printing details also refresh the flag from Scryfall on demand (details may be cached for one hour). Offline recognition-gallery candidates do not carry this metadata; their preview shows the badge once printing details load. No badge means no known positive flag, not a guarantee that the card is absent from the list. Deck summaries label each commander independently; the app does not store full decklists, so it cannot report a whole-deck Game Changer count.

There is one row per Scryfall `oracle_id`. The preferred printing is English, available on paper, non-digital, and non-promo, then the newest `released_at`; set code, collector number, and Scryfall UUID break ties. `default_cards` is used instead of `oracle_cards` because it provides printing images and lets the app choose that representative deterministically.

In **Deck details**, use **Choose printing** below the commander or partner to match the artwork on your card, then **Save deck**. **Use catalog default** removes the override. Printing choices load on demand from Scryfall (paper printings in all languages, with pagination), so browsing requires an internet connection. Printing metadata is cached separately in SQLite: saved artwork survives catalog refreshes and does not need another Scryfall API request to display. Image files use the shared cache described below. Changing a commander or partner clears that slot's printing; printing selection does not change commander identity, colors, imports, or statistics.

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

### Building the image yourself

```sh
docker build -t the-gathering .
```

Images are published to `ghcr.io/cfbender/the-gathering` by the [container workflow](.github/workflows/container.yml) on pushes to `main` and version tags. Published images are `linux/amd64` only; build locally (above) for other architectures.

## Development

The backend is a Rust server in [`rust/`](rust/README.md) (axum + sqlx on SQLite, the shared
[lotus](https://github.com/cfbender/lotus) crate for Scryfall and deck-list sources). The
toolchain (Rust, SQLite, Node, aube) is pinned in `mise.toml`. Install
[mise](https://mise.jdx.dev), then:

```sh
mise run setup                # tools, JavaScript dependencies, server build, demo data
mise run dev                  # Rust server on $PORT (4000) + Vite on http://localhost:5173
```

Open the Vite port: it serves the React app with hot reload and proxies API, socket, and page
requests to the server. The server applies any missing migrations to `the_gathering_dev.db` at
startup and syncs the Scryfall catalog when it is empty.

In development (`THE_GATHERING_ENV=dev`, set by `mise run dev`) every request is signed in
automatically as the first administrator (a passwordless `dev` admin is created if none exists)
and sudo re-authentication is skipped. Run with `DEV_AUTO_LOGIN=false` to exercise the real
login flow; `/login` stays reachable either way.

Other commands:

```sh
mise run rust:check                    # schema check, cargo fmt --check, clippy -D warnings, cargo test
mise run test                          # Rust tests only
mise run precommit                     # Rust checks + frontend checks, tests, and build
mise exec -- aube exec vp check        # frontend fmt + lint + typecheck
mise exec -- aube run build            # production frontend bundle
mise run rust:new-migration add_thing  # new rust/migrations/<timestamp>_add_thing.sql
mise run rust:sqlx-prepare             # after a migration or a query change
```

Layout: `rust/` (server, with the SQL migrations in `rust/migrations`), `assets/react`
(frontend), `priv/static` (static files). The version lives in `rust/crates/the-gathering/Cargo.toml`
and `package.json`; releases are cut by tagging `vX.Y.Z`. See [`AGENTS.md`](AGENTS.md) for
conventions.

## License

[Mozilla Public License 2.0](LICENSE).
