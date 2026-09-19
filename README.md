# The Gathering

A self-hosted tracker for Commander (Magic: The Gathering) games. Record who played, which commanders, who won, and how, then browse stats for your playgroup. One container, one SQLite file, no external services.

**Status:** early scaffold. The stack, container build, and developer setup are in place; game tracking features are next.

## Planned features

- Accounts with username/password; a server admin manages users and settings.
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
