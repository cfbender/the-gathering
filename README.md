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

### Deck-list links

`POST /api/decklists/resolve` accepts `{"url":"..."}` for a public Moxfield,
Archidekt, or self-hosted ManaVault deck (when `MANAVAULT_URL` is set). Successful responses contain the canonical
URL, deck name, commanders, commander color identity, author when exposed, card
count, and fetch timestamp under `data`. Successful lookups are cached in memory
for five minutes; errors are never cached.

The integrations use the upstream services' public interfaces:

- Moxfield: `GET https://api2.moxfield.com/v3/decks/all/:id`. The request sends a
  descriptive User-Agent, but Moxfield does not publish API limits or a supported
  third-party API contract and may reject server traffic with Cloudflare 403s.
- Archidekt: `GET https://archidekt.com/api/decks/:id/`. No authentication or
  documented public rate limit is currently required.
- ManaVault: `POST $MANAVAULT_URL/share/graphql` against the ManaVault instance you
  configure. No authentication is required; ManaVault's default limit is 120
  requests per IP per minute. Author is not exposed by its public schema. Only the
  configured origin is recognized; other origins are intentionally not fetched to
  avoid SSRF. Without `MANAVAULT_URL`, ManaVault links are stored as plain deck links.

### Environment variables

| Variable | Default | Purpose |
| --- | --- | --- |
| `SECRET_KEY_BASE` | required | Signs sessions and cookies. |
| `DATA_DIR` | `/data` | Where the database and files live. |
| `DATABASE_PATH` | `$DATA_DIR/the_gathering.db` | SQLite database file. |
| `PHX_HOST` | `localhost` | Public hostname used in generated URLs. |
| `PHX_SCHEME` | `https` | Public scheme. |
| `PHX_URL_PORT` | `443` for https, `80` for http | Public port. |
| `PORT` | `4000` | Port the server binds inside the container. |
| `CATALOG_SYNC_INTERVAL_HOURS` | `168` | Hours between automatic Scryfall catalog refreshes. |
| `MANAVAULT_URL` | unset | Origin of a self-hosted ManaVault instance whose shared deck links are recognized and resolved. |
| `DISCORD_CLIENT_ID` | unset | Discord application client ID; enables member OAuth sign-in when paired with the secret. |
| `DISCORD_CLIENT_SECRET` | unset | Discord application client secret. |
| `DISCORD_BOT_TOKEN` | unset | Discord bot token; enables automatic recording of completed SpellBot games when set. |
| `DISCORD_GUILD_ID` | unset | Optional development/server ID for immediate guild-scoped `/won` registration; without it the command is global. |
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

Removing a user disables the account rather than deleting it, preserving references from game
history. Disabled accounts cannot sign in and can be re-enabled by an administrator.

Sessions use random tokens stored in the `users_tokens` table, following Phoenix's generated-auth
design. Changing the administrator password expires every existing session. Sensitive actions
require authentication within the previous ten minutes; the SPA prompts the administrator for a
password and Discord members to authorize with Discord again. Passwords must be 12–72 characters.

See [Discord integration](docs/discord-integration.md) for bot creation, permissions, and current tracking behavior.
See [CSV game import](docs/csv-import.md) for the spreadsheet format, Mythic Track compatibility, and admin import flow.

### Card catalog

The app downloads Scryfall's compressed `default_cards` JSONL feed when the catalog is empty and refreshes it weekly. The response is streamed to a temporary file and decoded incrementally, then a complete staged generation is published atomically. Search uses only SQLite after sync; run a refresh manually with:

```sh
mise exec -- mix the_gathering.catalog.sync
```

There is one row per Scryfall `oracle_id`. The preferred printing is English, available on paper, non-digital, and non-promo, then the newest `released_at`; set code, collector number, and Scryfall UUID break ties. `default_cards` is used instead of `oracle_cards` because it provides printing images and lets the app choose that representative deterministically.

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
