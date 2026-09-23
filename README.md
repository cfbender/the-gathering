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
2. Copy `.env.example` to `.env` and set `SECRET_KEY_BASE`:

   ```sh
   openssl rand -base64 64 | tr -d '\n'
   ```

   Set `PHX_HOST`, `PHX_SCHEME`, and `PHX_URL_PORT` to the public address users reach the app at (for example `games.example.com`, `https`, `443` behind a reverse proxy).
3. Start it:

   ```sh
   docker compose up -d
   ```

The app listens on port 4000 and stores its SQLite database and files under `./data` (mounted at `/data`). Back up that directory to back up everything. TLS is left to your reverse proxy.

Health check: `GET /api/health` returns `{"status":"ok"}` when the database is reachable.

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

Each user can also save a Moxfield username, Archidekt username, and ManaVault
instance URL plus personal API key under **Settings → Profile → Deck hosts**.
`PATCH /api/session/user` accepts these as `moxfield_username`, `archidekt_username`,
`manavault_url`, and `manavault_api_key`. The API key is write-only: it is stored
encrypted with `SECRET_KEY_BASE`, never returned (user JSON exposes only
`has_manavault_api_key`), a blank value keeps the saved key, and `null` removes it.
`GET /api/session/remote-decks` returns the signed-in user's normalized public decks
(`name`, commanders, color identity, URL, source, and upstream update time) plus a
status for each source. Results, including source errors, are cached in memory for
five minutes. The user's player page links these decks, and the new-game form can
resolve one into a new deck with a quick pick.

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

**Games → Play** opens a temporary room for up to four signed-in members who share their
board cameras over a WebRTC mesh and record the result when the game ends. Clicking a card on
any board identifies it with the recognizer bundle published from `ml/` (see
`ml/README.md`, "Shipping"): Phoenix serves `DATA_DIR/cardid/current/*` at `/api/cardid/*`
and the browser runs the models itself. A recognized card opens with its rules text (fetched
from Scryfall per printing and cached) and lands in that board's card tray for every seat.
Without a published bundle the table still works and offers the player's commanders as
suggestions instead. Design notes are in
[docs/webcam-table.md](docs/webcam-table.md).

### Environment variables

| Variable | Default | Purpose |
| --- | --- | --- |
| `SECRET_KEY_BASE` | required | Signs sessions and cookies. |
| `DATA_DIR` | `/data` | Where the database, uploaded files, and the `cardid/` recognizer bundles live. |
| `DATABASE_PATH` | `$DATA_DIR/the_gathering.db` | SQLite database file. |
| `PHX_HOST` | `localhost` | Public hostname used in generated URLs. |
| `PHX_SCHEME` | `https` | Public scheme. |
| `PHX_URL_PORT` | `443` for https, `80` for http | Public port. |
| `PORT` | `4000` | Port the server binds inside the container. |
| `TRUST_PROXY_HEADERS` | unset | Set to `true` behind a reverse proxy so rate limiting identifies clients by `x-real-ip` / `x-forwarded-for` instead of the proxy address. |
| `LOG_LEVEL` | `info` | `debug`, `info`, `warning`, or `error`. `debug` explains why the Discord bot ignored a message. |
| `CATALOG_SYNC_INTERVAL_HOURS` | `168` | Hours between automatic Scryfall catalog refreshes. |
| `WEBRTC_STUN_URLS` | Google + Cloudflare public STUN | Comma-separated STUN URLs for webcam-table NAT discovery. Needed for any room that is not on one LAN; `none` disables STUN for LAN-only installs. |
| `WEBRTC_TURN_URLS` | unset | Comma-separated TURN URLs; with `WEBRTC_TURN_USERNAME` and `WEBRTC_TURN_CREDENTIAL`, relays webcam-table media when peers cannot connect directly. |
| `CLOUDFLARE_TURN_KEY_ID` | unset | With `CLOUDFLARE_TURN_API_TOKEN`, a Cloudflare Realtime TURN key. The server mints per-join credentials that expire after `CLOUDFLARE_TURN_TTL_SECONDS` (default 21600); relayed traffic is free up to 1,000 GB/month, then $0.05/GB. |
| `MANAVAULT_URL` | unset | Origin of a self-hosted ManaVault instance whose shared deck links are recognized and resolved. |
| `MANAVAULT_ALLOWED_HOSTS` | unset | Comma-separated exact hostnames allowed for personal ManaVault listing on private networks; also permits HTTP for those hosts. |
| `MANAVAULT_ALLOW_INSECURE_URLS` | unset | Set to `true` to permit HTTP personal ManaVault origins that resolve to public addresses. |
| `DISCORD_CLIENT_ID` | unset | Discord application client ID; enables member OAuth sign-in when paired with the secret. |
| `DISCORD_CLIENT_SECRET` | unset | Discord application client secret. |
| `DISCORD_BOT_TOKEN` | unset | Discord bot token; enables automatic recording of completed SpellBot games when set. |
| `DISCORD_GUILD_ID` | unset | Optional server ID for immediate guild-scoped `/won` and `/summary` registration; without it commands are global. Restricts summaries to that server when set. |
| `DISCORD_SPELLBOT_USER_ID` | `725510263251402832` | Discord user ID accepted as SpellBot, useful when running a private SpellBot deployment. |
| `THE_GATHERING_ADMIN_USERNAME` | unset | Creates this admin on container startup when paired with the password. |
| `THE_GATHERING_ADMIN_PASSWORD` | unset | Password for container or Mix-task admin bootstrap. |

### Accounts and registration

The first account registered in the browser becomes the server administrator and is the only
password account. After bootstrap, members sign in with Discord. New Discord identities are
accepted only while **Open registration** is enabled under **Admin → Users**; existing linked
members can always sign in. This flag is stored in the singleton `server_settings` row.

For headless container bootstrap, set `THE_GATHERING_ADMIN_USERNAME` and
`THE_GATHERING_ADMIN_PASSWORD`. Startup creates the admin if absent and is idempotent on later
restarts. In a source checkout, the equivalent task is:

```sh
THE_GATHERING_ADMIN_PASSWORD='use-a-long-password' mix the_gathering.create_admin USERNAME
```

**Detailed statistics from** on the same page sets a cutoff date for statistics that depend on
data your pod may not have recorded from the start: games before it still count toward wins and
losses, but their seat positions, game length, turn counts, and MVP cards are left out. See
[Statistics API](docs/stats.md).

Administrators can disable and re-enable accounts, or permanently delete an account. Disabling an
account signs it out on every device; re-enabling it does not revive those sessions, so the user
must sign in again. Administrators can also sign an account out everywhere without changing whether
it is enabled. Deleting an account revokes its sessions and unlinks its player while preserving the
player, decks, and game history. A returning Discord member can register again and reclaim the player
linked to the same Discord identity.

Sessions use random tokens stored in the `users_tokens` table, following Phoenix's generated-auth
design. Signing out ends only the current device's session. Changing the administrator password
expires every session for that account, and expired session rows are pruned when a new session is
issued. Sensitive actions—including import commits and player merges—require authentication within
the previous ten minutes; previews and sample downloads remain available without reauthentication.
The SPA prompts the administrator for a password and Discord members to authorize with Discord
again. Passwords must be 12–72 characters.

Password login and bootstrap registration (`POST /api/session`, `POST /api/users`) are limited
to 10 attempts per client address every 5 minutes; further attempts get `429 Too Many Requests`
with a `retry-after` header. Behind a reverse proxy every request arrives from the proxy's
address, so set `TRUST_PROXY_HEADERS=true` when your proxy sets `x-real-ip` or
`x-forwarded-for`. Password reauthentication is separately limited by account and client address,
with a server-wide attempt budget. Discord sign-in is not rate limited here.

See [Discord integration](docs/discord-integration.md) for bot creation, permissions, and current tracking behavior.
See [CSV game import](docs/csv-import.md) for the spreadsheet format and admin import flow, and [Mythic Track import](docs/mythic-track-import.md) for moving an existing Mythic Track playgroup over.

### Card catalog

The app downloads Scryfall's compressed `default_cards` JSONL feed when the catalog is empty and refreshes it weekly. The response is streamed to a temporary file and decoded incrementally, then a complete staged generation is published atomically. Search uses only SQLite after sync; run a refresh manually with:

```sh
mise exec -- mix the_gathering.catalog.sync
```

After each sync (and after every CSV or Mythic Track import) the app links decks and MVP cards that only carry a card name to catalog cards by name, filling in Scryfall IDs and missing colour identities. Trigger that alone from **Admin → Users → Link imported cards to the catalog** or with `mise exec -- mix the_gathering.catalog.backfill`.

There is one row per Scryfall `oracle_id`. The preferred printing is English, available on paper, non-digital, and non-promo, then the newest `released_at`; set code, collector number, and Scryfall UUID break ties. `default_cards` is used instead of `oracle_cards` because it provides printing images and lets the app choose that representative deterministically.

In **Deck details**, use **Choose printing** below the commander or partner to match the artwork on your card, then **Save deck**. **Use catalog default** removes the override. Printing choices load on demand from Scryfall (paper printings in all languages, with pagination), so browsing requires an internet connection. Printing metadata is cached separately in SQLite: saved artwork survives catalog refreshes and does not need another Scryfall API request to display. Image files still load from Scryfall's image CDN. Changing a commander or partner clears that slot's printing; printing selection does not change commander identity, colors, imports, or statistics.

A card can be a commander when it is a legendary creature or its oracle text says it can be your commander. Backgrounds are deliberately excluded. Partner, Partner with, Friends forever, Choose a Background, and Background are stored as a separate pairing classification for deck-building interfaces.

### Building the image yourself

```sh
docker build -t the-gathering .
```

Images are published to `ghcr.io/cfbender/the-gathering` by the [container workflow](.github/workflows/container.yml) on pushes to `main` and version tags.

## Development

The toolchain (Erlang, Elixir, Node, aube) is pinned in `mise.toml`. Install [mise](https://mise.jdx.dev), then:

```sh
mise install
mise exec -- mix setup        # deps, database, first asset build
mise exec -- mix phx.server   # http://localhost:5173
```

`mix phx.server` starts Phoenix on `$PORT` (default 4000) and the Vite dev server on 5173. Open the Vite port: it serves the React app with hot reload and proxies API and page requests to Phoenix.

In development every request is signed in automatically as the first administrator (a passwordless `dev` admin is created if none exists) and sudo re-authentication is skipped. Run with `DEV_AUTO_LOGIN=false` to exercise the real login flow; `/login` stays reachable either way.

Other commands:

```sh
mise exec -- mix test
mise exec -- mix precommit             # everything CI runs
mise exec -- aube exec vp check        # frontend fmt + lint + typecheck
mise exec -- aube run build            # production frontend bundle
```

Layout: `lib/the_gathering` (domain), `lib/the_gathering_web` (API and SPA shell), `assets/react` (frontend), `priv/repo/migrations`, `test`. See [`AGENTS.md`](AGENTS.md) for conventions.

## License

[Mozilla Public License 2.0](LICENSE).
