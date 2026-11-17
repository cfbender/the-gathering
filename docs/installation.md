# Installation

Everything needed to run The Gathering for a playgroup, from an empty host to members signing
in. Each step lists a command to run and a check that proves it worked, so a person or an
agent can follow it top to bottom.

The server is one Rust binary that serves the web app, the API, the webcam table's video
server, and the optional Discord bot. All state lives in one directory: a SQLite database plus
caches it can rebuild.

## 1. Decide before installing

Answer these first; the rest of the guide branches on them.

| Question | Why it matters | Default if unsure |
| --- | --- | --- |
| **Where does it run?** Docker host, Proxmox VE host, or a plain Linux server? | Picks the install method below. | Docker Compose |
| **What address will players use?** | Becomes `THE_GATHERING_PUBLIC_URL`. Sign-in cookies, invite links, and the Discord callback depend on it. | `http://<server-lan-ip>:4000` for a LAN-only trial |
| **Will players outside your home network join webcam tables?** | Video needs a UDP port range forwarded, or a TURN relay. | No (LAN only) |
| **Will members sign in with Discord?** | Without Discord only the single administrator account can sign in. | Yes for a real playgroup |
| **Do you want the Discord bot?** (`/newgame` queues, SpellBot tracking, `/log`, `/summary`) | Needs a bot token. | Optional |

Everything else (AI rulings, ManaVault, deck hosts, card recognition) is optional and can be
added later without reinstalling.

## 2. Requirements

| | Minimum | Notes |
| --- | --- | --- |
| CPU / architecture | x86-64 (amd64), 2 cores | Published images and tarballs are amd64 only; [build the image](#building-the-image-yourself) for arm64. |
| Memory | 1 GiB free, 2 GiB recommended | The weekly Scryfall catalog sync is the peak. |
| Disk | 2 GiB for data | Database (~300 MB once the card catalog syncs), card-recognition bundles (~180 MB), image cache (capped at 512 MiB). |
| Outbound internet | HTTPS | `api.scryfall.com`, `data.scryfall.io`, `cards.scryfall.io` (card data and images), `api.github.com` and `github.com` (recognizer bundles, update checks), `magic.wizards.com` and `media.wizards.com` (Comprehensive Rules), plus `discord.com` and `openrouter.ai` when those features are on. |
| Inbound | TCP 4000 (or your reverse proxy's 443) | Plus UDP `50000-50100` for webcam video from outside the LAN. |
| Browsers | Current Chrome, Edge, Firefox, or Safari | Webcam tables need HTTPS (or `localhost`) because browsers only grant camera access to secure origins. A LAN `http://` address works for everything except cameras. |

## 3. Install

Pick one method. All three end with the server listening on port 4000.

### Option A: Docker Compose (recommended)

Requires Docker Engine with the Compose plugin (`docker compose version` prints a version).

```sh
mkdir -p ~/the-gathering && cd ~/the-gathering
curl -fsSLO https://raw.githubusercontent.com/cfbender/the-gathering/main/docker-compose.yml
curl -fsSL https://raw.githubusercontent.com/cfbender/the-gathering/main/.env.example -o .env

# Required: a random secret of at least 64 bytes. Keep it; changing it signs everyone out
# and makes stored API keys unreadable.
sed -i.bak "s|^THE_GATHERING_SECRET_KEY=.*|THE_GATHERING_SECRET_KEY=$(openssl rand -base64 64 | tr -d '\n')|" .env

# The address players use (no path, no trailing slash).
sed -i.bak "s|^THE_GATHERING_PUBLIC_URL=.*|THE_GATHERING_PUBLIC_URL=https://games.example.com|" .env
```

Optionally create the first administrator at startup by setting `THE_GATHERING_ADMIN_USERNAME`
and `THE_GATHERING_ADMIN_PASSWORD` (12 to 72 characters) in `.env`. The container creates the
account only if it does not exist yet, so leaving them set is harmless. Otherwise the app shows a
setup page (see [step 5](#5-create-the-administrator)).

Start it:

```sh
docker compose up -d
```

> Compose only passes variables listed under `environment:` in `docker-compose.yml`. When you
> set a variable from the [reference](#configuration-reference) that the file does not list
> (`RULES_SYNC_ENABLED`, `CATALOG_SYNC_INTERVAL_HOURS`, `WEBRTC_SFU_IPV6`, or
> `CLOUDFLARE_TURN_TTL_SECONDS`), add a matching
> `NAME: ${NAME:-}` line there too. After any `.env` change run `docker compose up -d`, which
> recreates the container; `docker compose restart` keeps the old environment.

Data lives in `./data` next to the compose file. The image tag `latest` follows `main`; for releases, change the tag on the `image:` line
(pin `1.0` or a `v1.0.0` tag for releases).

### Option B: Proxmox VE LXC

Run as root on the Proxmox host. The script creates an unprivileged Debian 13 container, installs
the latest release tarball natively (no Docker), and runs it as a systemd service.

```sh
bash -c "$(curl -fsSL https://raw.githubusercontent.com/cfbender/the-gathering/main/deploy/proxmox/the-gathering.sh)"
```

From a terminal it asks for the public URL, the first administrator, and an automatic-update
schedule. To run it unattended, set them as environment variables (stdin that is not a terminal
skips every question):

```sh
PUBLIC_URL=https://games.example.com \
ADMIN_USERNAME=admin ADMIN_PASSWORD='a-long-password' \
AUTO_UPDATE='0 4 * * *' \
CTID=120 STORAGE=local-lvm CORES=4 RAM_MB=2048 \
bash -c "$(curl -fsSL https://raw.githubusercontent.com/cfbender/the-gathering/main/deploy/proxmox/the-gathering.sh)" </dev/null
```

Every container setting (`CTID`, `CT_HOSTNAME`, `STORAGE`, `DISK_GB`, `IP`, `GATEWAY`, `PIN_IP`,
`TIMEZONE`, `VERSION`, `TRUSTED_PROXIES`, and more) is documented at the top of
[`deploy/proxmox/the-gathering.sh`](../deploy/proxmox/the-gathering.sh). Inside the container:

| Path | Contents |
| --- | --- |
| `/etc/the-gathering.env` | Settings, same keys as `.env.example`; the secret is generated for you. |
| `/var/lib/the-gathering` | Data directory (database, caches). |
| `/opt/the-gathering/current` | The running release; the previous one stays in `releases/` for rollback. |
| `the-gathering.service` | `pct exec <CTID> -- systemctl restart the-gathering` after editing settings; logs with `journalctl -fu the-gathering`. |

### Option C: Release tarball on any Linux host

For a server without Docker. Needs x86-64 Linux with glibc 2.38 or newer (Debian 13, Ubuntu
24.04, Fedora 39, or later) and the DejaVu fonts (`fonts-dejavu-core`) for game summary images.

```sh
VERSION=v1.0.0   # or the newest tag on https://github.com/cfbender/the-gathering/releases
sudo useradd --system --home-dir /var/lib/the-gathering --shell /usr/sbin/nologin the-gathering
sudo mkdir -p /opt/the-gathering /var/lib/the-gathering
sudo chown the-gathering: /var/lib/the-gathering
cd /tmp
curl -fsSLO "https://github.com/cfbender/the-gathering/releases/download/$VERSION/the_gathering-$VERSION-linux-amd64.tar.gz"
curl -fsSLO "https://github.com/cfbender/the-gathering/releases/download/$VERSION/the_gathering-$VERSION-linux-amd64.tar.gz.sha256"
sha256sum -c "the_gathering-$VERSION-linux-amd64.tar.gz.sha256"
sudo tar -xzf "the_gathering-$VERSION-linux-amd64.tar.gz" -C /opt/the-gathering --strip-components=1

sudo curl -fsSL https://raw.githubusercontent.com/cfbender/the-gathering/main/.env.example -o /etc/the-gathering.env
sudo sed -i "s|^THE_GATHERING_SECRET_KEY=.*|THE_GATHERING_SECRET_KEY=$(openssl rand -base64 64 | tr -d '\n')|; s|^THE_GATHERING_PUBLIC_URL=.*|THE_GATHERING_PUBLIC_URL=https://games.example.com|" /etc/the-gathering.env
sudo chown root:the-gathering /etc/the-gathering.env && sudo chmod 640 /etc/the-gathering.env
```

Create the administrator (idempotent), then install the service:

```sh
sudo -u the-gathering bash -c 'set -a; . /etc/the-gathering.env; set +a
  DATA_DIR=/var/lib/the-gathering \
  THE_GATHERING_ADMIN_USERNAME=admin THE_GATHERING_ADMIN_PASSWORD="a-long-password" \
  /opt/the-gathering/bin/the-gathering bootstrap-admin'

sudo tee /etc/systemd/system/the-gathering.service >/dev/null <<'EOF'
[Unit]
Description=The Gathering
After=network-online.target
Wants=network-online.target

[Service]
User=the-gathering
Group=the-gathering
EnvironmentFile=/etc/the-gathering.env
Environment=THE_GATHERING_ENV=prod PORT=4000 DATA_DIR=/var/lib/the-gathering
ExecStart=/opt/the-gathering/bin/the_gathering start
Restart=on-failure
NoNewPrivileges=true
# The catalog sync stages downloads in /tmp, which ProtectSystem=strict would make read-only.
PrivateTmp=true
ProtectSystem=strict
ReadWritePaths=/var/lib/the-gathering

[Install]
WantedBy=multi-user.target
EOF
sudo systemctl daemon-reload && sudo systemctl enable --now the-gathering
```

To update later, download and check the newer tarball the same way, then replace the directory:
`sudo systemctl stop the-gathering && sudo rm -rf /opt/the-gathering/* && sudo tar -xzf <tarball> -C /opt/the-gathering --strip-components=1 && sudo systemctl start the-gathering`.
Migrations run automatically at boot.

## 4. Verify the server

Run these against the server itself (replace the host when checking from elsewhere):

```sh
curl -fsS http://localhost:4000/api/health
# {"status":"ok"}

curl -fsS http://localhost:4000/api/registration
# {"data":{"allowed":false,"bootstrap":true,"discord_configured":false}}
#   bootstrap:true           -> no administrator yet; finish step 5
#   discord_configured:true  -> Discord sign-in is set up (step 6)
```

The startup log says what is on and off. Read it with `docker compose logs -f the-gathering`,
`journalctl -fu the-gathering`, or `pct exec <CTID> -- journalctl -fu the-gathering`. Expect
`listening on http://[::]:4000`, a line saying whether Discord OAuth sign-in is enabled, and
shortly after, `installed the Comprehensive Rules`. At first start the server also downloads the
Scryfall card catalog (several hundred MB, a few minutes); card search and images fill in once
it finishes, and **Admin → Catalog** shows its state.

## 5. Create the administrator

If you did not set the administrator variables in step 3, open the public URL in a browser.
With no accounts yet, the app sends you to **/register**: the first account created there is the
administrator, and the only password account the browser can create. Afterwards
`/api/registration` reports `"bootstrap":false`.

## 6. Let members sign in (Discord)

Members sign in with Discord. One Discord application covers both sign-in and the optional bot.

1. In the [Discord Developer Portal](https://discord.com/developers/applications), create an
   application. Optionally upload [`priv/static/images/discord-app-icon.png`](../priv/static/images/discord-app-icon.png)
   as its icon.
2. **OAuth2 → Redirects**: add `<THE_GATHERING_PUBLIC_URL>/auth/discord/callback`, for example
   `https://games.example.com/auth/discord/callback`. It must match exactly.
3. Copy the **Client ID** and **Client Secret** into `DISCORD_CLIENT_ID` and
   `DISCORD_CLIENT_SECRET`, then restart (`docker compose up -d` or `systemctl restart`).
4. Check: the log shows `Discord OAuth sign-in enabled; redirect URI is …` (with the URL from
   step 2), and `/api/registration` shows `"discord_configured":true`.
5. Signed in as the administrator, turn on **Admin → Settings → Open registration** so new Discord
   accounts are accepted, or send members a playgroup invite link from **Admin → Playgroups**.
   Turn open registration off again once everyone has joined.

**Optional bot.** On **Bot**, reset the token, copy it into `DISCORD_BOT_TOKEN`, and enable
**Message Content Intent**. Invite the bot to your Discord server with this URL (replace the ID):

```text
https://discord.com/oauth2/authorize?client_id=YOUR_APPLICATION_ID&scope=bot%20applications.commands&permissions=326686002176
```

Set `DISCORD_GUILD_ID` to your server's ID so slash commands appear immediately (global
registration can take an hour), then restart. The log should show
`registered /log, /summary, /newgame, /close, and /alerts in guild …`. SpellBot tracking, the
`/newgame` queue, and per-command permissions are covered in
[Discord integration](discord-integration.md).

## 7. HTTPS and the reverse proxy

Run a reverse proxy that terminates TLS in front of port 4000, and keep port 4000 itself off the
public internet. The proxy must pass WebSocket upgrades (the webcam table uses Socket.IO on
`/socket.io/`) and must not buffer Server-Sent Events (**Admin → Server logs**).

Set these with the proxy in place:

```sh
THE_GATHERING_PUBLIC_URL=https://games.example.com
TRUST_PROXY_HEADERS=true            # rate limits see real client addresses
TRUSTED_PROXIES=192.168.1.20        # the proxy's address; blank trusts loopback and private ranges
```

Once the public URL is `https://` on a real host name, the session cookie is `Secure`: signing in
over `http://<host>:4000` stops working, so always use the public address. Add HSTS on the proxy.

Caddy (`Caddyfile`) handles certificates and WebSockets with no extra settings:

```caddyfile
games.example.com {
	reverse_proxy 127.0.0.1:4000
}
```

nginx:

```nginx
server {
    listen 443 ssl http2;
    server_name games.example.com;
    # ssl_certificate ...; ssl_certificate_key ...;
    client_max_body_size 10m;

    location / {
        proxy_pass http://127.0.0.1:4000;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}
```

## 8. Webcam video from outside your network

Video does not go through the reverse proxy. Each browser sends UDP straight to the server's
built-in video server (SFU). Choose one setup:

| Setup | Do this | Check |
| --- | --- | --- |
| **LAN only** | Nothing. Optionally `WEBRTC_STUN_URLS=none`. | Two browsers on the LAN see each other's camera. |
| **Port forward** (best quality, no relay cost) | Forward UDP `50000-50100` on the router to the server (Docker already publishes it), and set `WEBRTC_SFU_PUBLIC_IP` to the router's public IPv4 address. | A player on mobile data sees your board; **Table → Connection** shows a direct transport. |
| **No port forwarding possible** | Create a TURN key in the Cloudflare dashboard (Realtime → TURN), set `CLOUDFLARE_TURN_KEY_ID`, `CLOUDFLARE_TURN_API_TOKEN`, and `WEBRTC_SFU_RELAY_ONLY=true`. In Docker, remove the UDP `ports:` line. | **Table → Connection** says "via TURN relay". Relayed traffic is free up to 1,000 GB a month, then $0.05/GB (about 20 GB per 1080p player in a three-hour game). |

Each connected browser uses one port from `WEBRTC_SFU_PORT_RANGE`; widen the range (and the
forward and the compose `ports:` line together) for more simultaneous players. Players on
networks that block UDP entirely also need a TURN server (`WEBRTC_TURN_URLS`,
`WEBRTC_TURN_USERNAME`, `WEBRTC_TURN_CREDENTIAL`, or the Cloudflare key above). The details are in
[Webcam table design](webcam-table.md#ice-stun-turn-and-proxies).

## 9. Optional features

| Feature | Enable | Where |
| --- | --- | --- |
| **AI rulings** | An [OpenRouter](https://openrouter.ai) API key and a tool-calling model ID | **Admin → Settings → AI rulings** (no environment variable; stored encrypted). Set a monthly per-member spend cap there. |
| **Card recognition** | On by default: the server downloads the newest `scanner-bundle-*` release at boot and every six hours | `CARDID_BUNDLE_SOURCE=off` disables downloads. Without a bundle, clicking a card suggests the player's commanders instead. |
| **ManaVault deck links** | `MANAVAULT_URL=https://manavault.example.com` | Private-network instances also need `MANAVAULT_ALLOWED_HOSTS`. |
| **Update button in the admin UI** | Docker: `COMPOSE_PROFILES=self-update` and `WATCHTOWER_HTTP_API_TOKEN=$(openssl rand -hex 32)` in `.env`, then `docker compose up -d`. Proxmox: already wired up. | **Admin → Server settings → Software update** |
| **Import history** | Nothing to configure | **Import** in the navigation: [CSV](csv-import.md), [Mythic Track](mythic-track-import.md), or another instance's export. |
| **Several playgroups** | Nothing to configure | **Admin → Playgroups**. To serve several Discord servers from one bot, leave `DISCORD_GUILD_ID` unset and set each server's ID on its playgroup. |

## 10. Final checklist

An install is finished when all of these hold:

- [ ] `GET <public URL>/api/health` returns `{"status":"ok"}` through the reverse proxy.
- [ ] `GET <public URL>/api/registration` returns `"bootstrap":false` (the administrator exists).
- [ ] The administrator can sign in at `<public URL>/login`.
- [ ] If Discord is configured: `"discord_configured":true`, and a second account can sign in with Discord after **Open registration** is on or through an invite link.
- [ ] **Admin → Catalog** shows a successful sync, and card search finds "Sol Ring".
- [ ] **Games → Play** opens a table, and a second device (off your network, if remote play is wanted) sees the first camera.
- [ ] The data directory is in your backups (below).

## Backups, updates, and moving servers

**Back up** the data directory (`./data` for Docker, `/var/lib/the-gathering` otherwise) and the
settings file (`.env` or `/etc/the-gathering.env`; it holds the secret key). `card-images/` and
`cardid/` are caches and can be skipped. For a consistent copy while the server runs:

```sh
sqlite3 data/the_gathering.db ".backup 'the_gathering-$(date +%F).db'"
```

**Update**: Docker, `docker compose pull && docker compose up -d`; Proxmox, `update` inside the
container (or the admin UI's **Update now**, or the cron schedule); tarball, unpack the new
release and restart. Database migrations run at boot and cannot be undone by installing an older
release, so back up before updating. **Admin → Server settings → Software update** shows the
running version and whether a newer build exists.

**Move** to a new host by installing there, stopping both servers, copying the data directory
and the settings file (same `THE_GATHERING_SECRET_KEY`), and starting the new one. A Docker
settings file works for the Proxmox install and the reverse; the Proxmox service pins its own
paths and port.

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
  started from (`latest` follows `main`; pin `1.0` or a `v1.0.0` tag for releases) and recreate
  the container. Watchtower does not poll on its own in this setup.

When neither is configured the section only shows the version and says the server has to be
updated by hand. The version comes from `priv/VERSION`, which the release and container builds
write; development builds have none.

### Proxmox update channels

Updates work like the community-scripts helpers: inside the container, `update [tag]` is on `PATH`
and installs the latest (or given) release and restarts the service (it runs the current copy of
the script from GitHub, so fixes to the updater reach existing containers). From the Proxmox host,
`bash the-gathering.sh update <CTID> [tag]` does the same, as does **Update now** in the admin UI
(see above; it logs to `journalctl -t the-gathering-update`). The previous release stays under
`/opt/the-gathering/releases/` for rollback. Release tarballs are built by
[`.github/workflows/release.yml`](../.github/workflows/release.yml) for every `v*.*.*` tag, and every
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

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| Server exits with `THE_GATHERING_SECRET_KEY is missing` or `must be at least 64 bytes` | Generate one with `openssl rand -base64 64 \| tr -d '\n'`. |
| `THE_GATHERING_PUBLIC_URL must not have a path` | The app is served at the root of its origin; use `https://games.example.com`, not `https://example.com/games`. |
| Sign-in form accepts the password but you land back on the login page | The public URL is `https://` but you are browsing over `http://`. Use the public URL. |
| Login page says **Discord sign-in is not configured** | Both `DISCORD_CLIENT_ID` and `DISCORD_CLIENT_SECRET` must reach the server. With Docker, run `docker compose exec the-gathering env \| grep -c DISCORD_CLIENT` (should print `2`) and recreate with `docker compose up -d`. |
| Discord sign-in fails with a redirect error | The redirect in the Discord portal must equal `<THE_GATHERING_PUBLIC_URL>/auth/discord/callback`, as printed at startup. |
| "Registration is closed" for a new member | Turn on **Admin → Settings → Open registration**, or send an invite link from **Admin → Playgroups**. |
| Webcam tiles stay at "Connecting…" for remote players | The UDP range is not reachable. Forward it and set `WEBRTC_SFU_PUBLIC_IP`, or switch to relay mode ([step 8](#8-webcam-video-from-outside-your-network)). |
| Browser never asks for the camera | The page is not on HTTPS (or `localhost`). |
| Card search is empty | The first catalog sync is still running or failed. **Admin → Catalog** shows its state; **Sync now** retries. Needs outbound HTTPS to Scryfall. |
| `429 Too Many Requests` for everyone behind the proxy | Set `TRUST_PROXY_HEADERS=true` and `TRUSTED_PROXIES` to the proxy's address. |
| More detail needed | `LOG_LEVEL=debug`, then read the log or **Admin → Server logs**. |

## Configuration reference

Every variable the server reads. Only `THE_GATHERING_SECRET_KEY` is required.

| Variable | Default | Purpose |
| --- | --- | --- |
| `THE_GATHERING_SECRET_KEY` | required | Encrypts the session cookie, socket tokens, stored ManaVault API keys, and the OpenRouter API key. At least 64 bytes; keep it when upgrading, or stored keys stop decrypting. Releases up to 0.2 called it `SECRET_KEY_BASE`, which still works and logs a warning. |
| `THE_GATHERING_PUBLIC_URL` | `https://localhost` | Origin users reach the app at (scheme, host, and a non-default port), used for generated links and the Discord OAuth redirect. An `https://` origin other than `localhost` also marks the session cookie `Secure`. Replaces `PHX_SCHEME`, `PHX_HOST`, and `PHX_URL_PORT`, which still work and log a warning. |
| `DATA_DIR` | `/data` | Where the database, uploaded files, and the `cardid/` recognizer bundles live. |
| `DATABASE_PATH` | `$DATA_DIR/the_gathering.db` | SQLite database file. |
| `PORT` | `4000` | Port the server binds. |
| `TRUST_PROXY_HEADERS` | unset | Set to `true` behind a reverse proxy so rate limiting identifies clients by `x-real-ip` / `x-forwarded-for` instead of the proxy address. The headers are believed only from `TRUSTED_PROXIES`. |
| `TRUSTED_PROXIES` | loopback and private ranges | Comma-separated addresses or CIDR ranges (for example `192.168.1.20` or `172.16.0.0/12`) whose forwarded headers are believed when `TRUST_PROXY_HEADERS` is on. Narrow it to your proxy when untrusted clients share its private network. |
| `LOG_LEVEL` | `info` | `debug`, `info`, `warning`, or `error`. `debug` explains why the Discord bot ignored a message. |
| `THE_GATHERING_ADMIN_USERNAME` | unset | With the password, creates this administrator at container start (Docker) or with `the-gathering bootstrap-admin`; does nothing once it exists. |
| `THE_GATHERING_ADMIN_PASSWORD` | unset | Password for that administrator, 12 to 72 characters. |
| `CATALOG_SYNC_INTERVAL_HOURS` | `168` | Hours between automatic Scryfall catalog refreshes. |
| `RULES_SYNC_ENABLED` | `true` | Set to `false` to stop downloading the Comprehensive Rules for AI rulings and checking daily for new releases. |
| `CARDID_BUNDLE_SOURCE` | `github` | Where the card-recognition bundle is downloaded from, at boot and every six hours: `github` installs the newest published `scanner-bundle-*` release of this repository, an `https://.../manifest.json` URL installs the bundle beside it, and `off` only serves bundles copied in by hand. |
| `CARDID_CORRECTIONS_TOKEN`, `CARDID_CORRECTIONS_ADMIN_ID` | unset | Optional read-only export of card-recognition corrections for training, bound to an administrator's user ID. |
| `WEBRTC_SFU_PORT_RANGE` | `50000-50100` | UDP ports the built-in webcam-table SFU listens on, one per connected browser. Forward the same range from your router to the host (Docker: publish it as `/udp`). |
| `WEBRTC_SFU_PUBLIC_IP` | unset | Public address browsers reach the SFU ports at, announced as an ICE candidate. Required for players outside your LAN unless `WEBRTC_SFU_RELAY_ONLY` is set. |
| `WEBRTC_SFU_IPV6` | unset | Set to `true` to also offer the host's IPv6 addresses as media candidates. |
| `WEBRTC_SFU_RELAY_ONLY` | unset | Set to `true` to open no public ports: the SFU reaches browsers through Cloudflare TURN (requires `CLOUDFLARE_TURN_KEY_ID`), so all media crosses the relay. |
| `WEBRTC_STUN_URLS` | Google + Cloudflare public STUN | Comma-separated STUN URLs browsers use for NAT discovery. `none` disables STUN for LAN-only installs. |
| `WEBRTC_TURN_URLS`, `WEBRTC_TURN_USERNAME`, `WEBRTC_TURN_CREDENTIAL` | unset | A static TURN relay for browsers that cannot reach the SFU directly. |
| `CLOUDFLARE_TURN_KEY_ID`, `CLOUDFLARE_TURN_API_TOKEN` | unset | A Cloudflare Realtime TURN key. The server mints per-join credentials that expire after `CLOUDFLARE_TURN_TTL_SECONDS` (default 21600). |
| `MANAVAULT_URL` | unset | Origin of a self-hosted ManaVault instance whose shared deck links are recognized and resolved. |
| `MANAVAULT_ALLOWED_HOSTS` | unset | Comma-separated exact hostnames allowed for personal ManaVault listing on private networks; also permits HTTP for those hosts. |
| `MANAVAULT_ALLOW_INSECURE_URLS` | unset | Set to `true` to permit HTTP personal ManaVault origins that resolve to public addresses. |
| `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET` | unset | Discord application credentials; together they enable member sign-in. |
| `DISCORD_BOT_TOKEN` | unset | Enables the optional Discord bot: SpellBot tracking, `/log`, `/summary`, and `/newgame` webcam-table queues. |
| `DISCORD_GUILD_ID` | unset | Optional server ID for immediate guild-scoped slash-command registration; without it commands are global (needed when playgroups span several Discord servers). |
| `DISCORD_DEFAULT_TIMEZONE` | `America/New_York` | IANA time zone for `/newgame start:8pm` and `tomorrow 7pm`. |
| `DISCORD_NEWGAME_ROLE_ID` | unset | Optional role mentioned in each `/newgame` thread, which adds its members to the thread. Members toggle it with `/alerts`. |
| `DISCORD_SPELLBOT_USER_ID` | `725510263251402832` | Discord user ID accepted as SpellBot, for private SpellBot deployments. |
| `COMPOSE_PROFILES`, `WATCHTOWER_HTTP_API_TOKEN` | unset | Docker only: `self-update` and a token turn on the admin UI's **Update now** button through a Watchtower sidecar. |

## Maintenance commands

The server binary takes subcommands. In Docker prefix them with
`docker compose exec the-gathering /app/bin/the-gathering`; in the Proxmox container or a tarball
install run `/opt/the-gathering/current/bin/the-gathering` or `/opt/the-gathering/bin/the-gathering`
as the `the-gathering` user with the settings file loaded.

| Command | What it does |
| --- | --- |
| `migrate` | Applies pending database migrations (also done at every start). |
| `bootstrap-admin` | Creates `THE_GATHERING_ADMIN_USERNAME` as administrator unless it exists. |
| `create-admin USERNAME` | Creates another administrator with `THE_GATHERING_ADMIN_PASSWORD`. |
| `catalog-sync` | Downloads the Scryfall catalog now. |
| `catalog-backfill` | Links imported deck and MVP card names to catalog cards. |

## Building the image yourself

```sh
git clone https://github.com/cfbender/the-gathering && cd the-gathering
docker build -t the-gathering .
```

Then set `image: the-gathering` in `docker-compose.yml`. Published images are `linux/amd64`
only; a local build works for arm64. Developing on the code itself is covered in the
[README](../README.md#development).
