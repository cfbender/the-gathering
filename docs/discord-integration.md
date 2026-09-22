# Discord integration

This document records the September 2026 research behind the Discord client and
the intended boundary with the games domain.

## Member sign-in

Members authenticate with Discord OAuth. The first, administrator account is
bootstrapped with a username and password; after that, password registration is
disabled. An unknown Discord identity creates a member only while **Admin →
Users → Open registration** is enabled. Existing linked members can sign in
while registration is closed, but disabled accounts cannot.

Use the same Discord application for OAuth and the optional game-tracking bot:

1. In the [Discord Developer Portal](https://discord.com/developers/applications),
   open the application and copy its **Application ID** and OAuth2 client secret
   into `DISCORD_CLIENT_ID` and `DISCORD_CLIENT_SECRET`. For the **App Icon**,
   upload `priv/static/images/discord-app-icon.png`; it is the app logo on a
   full-bleed square because Discord applies its own circular crop.
2. Under **OAuth2**, register
   `<PHX_SCHEME>://<PHX_HOST>:<PHX_URL_PORT>/auth/discord/callback`. Omit the port
   when it is the scheme default (for example,
   `https://games.example.com/auth/discord/callback`). The application derives
   this URL from the Phoenix endpoint settings.
3. Restart the app. The login page shows **Continue with Discord** when both
   credentials are present. Authorization requests the `identify email` scopes;
   email is not persisted.

The callback links the Discord ID to both the account and its `players` row.

If the login page shows **Discord sign-in is not configured**, the running
container has no `DISCORD_CLIENT_ID`/`DISCORD_CLIENT_SECRET` pair. The app logs
`Discord OAuth sign-in enabled; redirect URI is …` or `Discord OAuth sign-in
disabled: …` at every start, and `docker compose exec the-gathering env | grep -c
DISCORD_CLIENT` should print `2`. Common causes:

- `docker-compose.yml` predates member sign-in and lacks the `DISCORD_CLIENT_ID`
  and `DISCORD_CLIENT_SECRET` passthrough lines; copy the current file from the
  repository. Compose only forwards variables listed under `environment`.
- `.env` was edited after the container was created. `docker compose restart`
  keeps the old environment; run `docker compose up -d` to recreate it.
- Only one of the two variables is set. The app warns about this at boot.

Sign-in failures redirect to `/login` with a one-time message; the message is
dropped from the URL once shown, so a member who was turned away while
registration was closed must click **Continue with Discord** again after the
administrator opens it (reloading the old page does not retry). The server logs
the cause of every rejected callback: `Discord sign-in rejected an unknown
account because registration is closed` at `info`, and `Discord sign-in failed:
…` (with Discord's error response) at `warning` for credential or redirect-URI
problems.
Discord reauthorization also supplies the ten-minute confirmation required for
sensitive actions. `DISCORD_CLIENT_SECRET` and `DISCORD_BOT_TOKEN` are separate
secrets even when they belong to the same application.

## Findings

### SpellBot starts games; it does not normally finish them

`/lfg` creates or joins a pending queue. When the queue fills (or `/start`
shrinks it), SpellBot creates the remote table, copies queued users to durable
`Play` rows, marks the game `STARTED`, and edits the existing queue message to
say **Your game is ready!**. The admin-only `/game` command creates a fully
seated game and follows the same start path. Relevant upstream code:

- [`/lfg` and `/game` command definitions](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/cogs/lfg_cog.py#L58-L114),
  [`/game`](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/cogs/events_cog.py#L27-L61)
- [queue-to-start transition](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/services/games.py#L651-L723)
- [the existing Discord post is edited](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/actions/lfg_action.py#L691-L785)

The core state machine has only `PENDING` and `STARTED`; `Game` has no winner or
finished-at column. `Play` stores the Discord user ID, game/guild IDs, timestamps,
a tracking PIN, and verification time—not a commander, deck, result, or points.
See the [`Game` model](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/models/game.py#L25-L190)
and [`Play` model](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/models/play.py#L24-L90).
SpellBot removed its experimental points/ELO data in 2025; `/record` is not a
score system ([cleanup migration](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/migrations/versions/d97abaf70fcf_delete_score_elo_record_cruft.py#L22-L28)).

After start, SpellBot reliably knows:

- SpellBot game ID, format, service, status, rules/bracket, and created/started
  timestamps;
- guild/channel/post IDs and each player's Discord ID and cached display name;
- the service table link (Convoke, Table Stream, and others), though it may be
  shown only in player DMs;
- arbitrary post-game metadata, but only when another tracker supplies it.

The ready embed exposes the roster, format, start timestamp, service, and game
ID. Its exact builder is [`GameData.to_embed`](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/data/game_data.py#L178-L267),
with players rendered as mentions and the footer rendered as
`SpellBot Game ID: #SB… — Service: …` ([players](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/data/game_data.py#L382-L420)).
The fixture in `test/fixtures/discord/spellbot_game_ready.json` is adapted from
SpellBot's [upstream embed assertion](https://github.com/lexicalunit/spellbot/blob/main/tests/cogs/test_lfg_cog.py#L104-L176)
with invented IDs and names.

SpellBot has a public per-game HTML page, but no documented public per-game JSON
read API or subscriber webhook. Its authenticated API lets approved integrations
`POST /api/game/{id}/verify`, `/record`, or `/metadata`; metadata can include
winner, commander, duration, turns, and tracker links. `/record` sends DMs but
does not persist its supplied winner/commanders. See [SpellBot's API documentation](https://github.com/lexicalunit/spellbot/blob/main/API.md#L156-L307)
and [REST handlers](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/web/api/rest.py#L250-L379).
The generic Castlog forwarding hook is configured by SpellBot itself, not a
public webhook-subscription API.

“Verified” means an individual player proved possession of their secret game
PIN. It does not mean the game result or winner was verified
([verification handler](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/services/plays.py#L357-L386)).

### Convoke and Mythic Track

[Convoke](https://convoke.games/en/what-is-convoke) is a hosted webcam Magic
table with life/game context, card recognition, and deck features. Its
[official Mythic Track partner page](https://convoke.games/en/partners/mythic-track)
says a player adds a Mythic Track API key in Convoke's Plugins UI; when selected
for a game, Convoke submits completed match data including Discord identity,
winner, win condition, mulligans, and key cards directly to Mythic Track.

No public Convoke developer API, webhook registration, plugin SDK, or payload
schema for third parties was found in its public site or documentation. The
available evidence describes a named partner integration, not a generally
available hook. A direct Convoke integration therefore requires a partnership
or private API access and is not an MVP dependency.

### Discord delivery options

| Option | Benefits | Costs / blockers |
| --- | --- | --- |
| Gateway bot | Sees SpellBot's message create/update events and receives slash commands; no public URL required. | Persistent WebSocket, bot token, `GUILDS` + `GUILD_MESSAGES`, and privileged `MESSAGE_CONTENT` intent because embeds are message content. |
| Interactions-only HTTP | No gateway or Message Content intent; Phoenix can verify Discord's Ed25519 signatures. | Requires a stable public HTTPS endpoint and cannot passively see SpellBot's roster. Discord requires every request signature to be verified. |
| Poll channel history | Can recover games after downtime. | Requires Read Message History, polling/state, rate-limit handling, and still needs Message Content access; it cannot discover winners. |

Discord documents the mutually exclusive [gateway and HTTP interaction delivery modes](https://docs.discord.com/developers/interactions/overview),
the public endpoint and Ed25519 requirements for HTTP interactions, and that
message `content`, `embeds`, `attachments`, and `components` require the
[Message Content intent](https://docs.discord.com/developers/events/gateway#message-content-intent).

The client uses [Nostrum 0.10.4](https://hex.pm/packages/nostrum/0.10.4), the
stable Elixir Discord gateway library. It is maintained, but its [published CI
matrix](https://github.com/Kraigie/nostrum/blob/master/.github/workflows/test_and_lint.yml#L51-L82)
does not cover this project's Elixir 1.20 / OTP 29 combination. The
skeleton compiles and its isolated tests run on OTP 29. Nostrum's current Gun /
Cowlib dependency set also has [published 2026 security advisories](https://hex.pm/packages/gun/advisories);
the gateway
does not construct headers from untrusted game data, but the dependency should
be upgraded when Nostrum publishes a patched stable release. These are reasons
to revisit the library choice before broad distribution, not reasons to write a
home-grown gateway state machine.

## Recommended MVP

**Primary:** run a gateway bot alongside Phoenix. Accept only messages authored
by the configured SpellBot user ID, then require the known started color, title,
footer, start timestamp, and player field. Emit an incomplete normalized report
when a game starts. A listed player finishes it with `/won`, which defaults to
the most recently started game the bot has seen in that channel, or
`/won game:SB12345` to pick a specific one; the reply is ephemeral, and a
non-player cannot report the game. A participant chooses the winner from the
roster and confirms the result after reviewing the details.

This fills the exact gap SpellBot leaves while avoiding screen-scraping its HTML
or depending on private Convoke APIs. It also keeps the integration usable for
SpellBot games launched on services other than Convoke.

**Fallback:** pursue a direct Convoke partnership. Its completion event has the
best data (winner, commanders/decks and richer outcomes), and could feed the same
`GameReport`/sink boundary. Until such access exists, a message-history backfill
can improve missed-roster recovery but cannot recover winners.

Reactions were rejected for the initial UX: they are easy to place on the wrong
message, add reaction intent/permission complexity, and still require a policy
for who may choose another player's winner. `/won` is explicit and attributable.

## Implemented flow

```text
SpellBot edits ready embed
        │
        ▼
Discord gateway (MESSAGE_UPDATE / MESSAGE_CREATE)
        │ author ID + embed contract validation
        ▼
GameReport (winner_discord_ids: []) ───────┐
        │ staged in SQLite                 │
        │                                  ▼
Player runs /won [game:SB12345]     pluggable Sink
        │                                  ▲
        ▼                                  │
membership check + private draft           │
        │ details, winner, kills, review    │
        └── Save → GameReport ─────────────┘
```

`GameReport` contains `external_id` (`spellbot:SB12345`), `source`
(`discord`), `played_at`, guild/channel IDs, players with Discord ID/display
name/nullable commander, winner Discord IDs, and scrub-safe raw embed data.
Commanders are `nil` because the ready embed does not contain them.

### Recording a result with `/won`

The command opens a modal for optional **turns, duration in minutes, winner's MVP
card, and notes**. Duration starts as an estimate from the SpellBot start time;
edit or clear it when reporting an older game. Submitting opens a private review
message with **winner** and **win-condition** dropdowns, **Edit details**,
**Player kills**, **Save game**, and **Cancel**. The reporter is the initial
winner, but any roster member can be selected. Win conditions use the same enum
as the web app; Unknown is available. Draws still use the web editor.

MVP names are resolved against the local card catalog. Ambiguous names show a
card-selection dropdown; an unmatched name must be corrected or cleared. MVP
belongs only to the selected winner. Notes allow up to 4,000 characters; the
review truncates its preview, not the saved note. Mentions in review messages do
not ping anyone.

The kills modal labels each field with a player name. Enter `0` for no kills or
leave it blank for unknown. Visit each kills page before saving, even when all
values are unknown. Discord allows five inputs per modal, so six-player games
have a **More player kills** page. The installed Nostrum version decodes legacy
ActionRow text inputs, so selection dropdowns live in the review message rather
than inside the modal.

Nothing is recorded until **Save game**. Closing a modal or cancelling leaves
the pending game intact. Drafts persist across restarts for one hour and are
bound to the reporter, server, channel, and original roster/start time. Changed
or expired drafts must be reopened. Saving consumes the pending game in the
same transaction as recording the result; competing drafts cannot overwrite it.
Already recorded games must be edited in the web app.

Winnerless reports are staged in SQLite, so `/won` continues to work after an
application or Tracker restart. Re-observing the same SpellBot external ID
updates its staged timestamp, roster, commander names, and scrub-safe normalized
data rather than creating a duplicate. Without a `game` option, `/won` queries
the latest staged report in the invoking channel that does not already have a
recorded Discord game with the same external ID. This skips games whose winners
were previously recorded but which a later SpellBot edit re-staged, and reports
that no game is available when every staged game in the channel is already
recorded. If the invoker was not in the latest winnerless game they are told to
pass the ID. Completed reports are persisted by the games sink described below
and removed from staging only after the sink succeeds.

Administrators can review staged reports under **Admin → Pending Discord games**,
which applies the same winnerless filter so re-staged games that already have a
recorded winner are hidden. They can choose any listed player as the winner, or
discard a report that should not be recorded. The corresponding sudo-protected
API is `GET /api/admin/discord/pending`, `PATCH
/api/admin/discord/pending/:id`, and `DELETE /api/admin/discord/pending/:id`.
Pending reports are retained for 30 days after their latest observation.
`TheGathering.Discord.StageReport` owns staging,
`ResolvePendingGame` owns resolution and discard, and the `Discord` context is
the entry point used by both Tracker and the admin API. Tracker explicitly
prunes stale rows after staging; reading the pending list never writes. Game
persistence and pending-row consumption occur in one transaction. This bounds
storage while leaving a month for `/won` or administrator recovery. The staged
data is normalized; raw Discord payloads are never stored in full or logged.

## Game tracking

The Discord supervisor uses `TheGathering.Discord.Sink.Games` by default. A
winnerless SpellBot start remains in durable staging rather than being recorded
as a draw. When a listed player confirms **Save game** in `/won`,
the sink creates or reuses players by Discord ID and records one `games` row
with `source: "discord"`, the SpellBot ID as `external_id`, and seats in the
order SpellBot listed them. The selected player is the winner and every other
seat is a loss. Win condition, turns, duration, notes, individual kills, and the
winner's resolved MVP are persisted with the result.

If a report supplies a commander, the sink creates or reuses a deck named for
that commander. It leaves `commander_card_id` unset and the color identity empty;
the ready embed currently supplies no commander, and this path does not query
the card catalog.

Repeated sink reports are idempotent by `{source, external_id}`. A completed replay
replaces the existing game's timestamp, seats, decks, and results, so corrected
seat order or winner data does not create a duplicate. The `/won` flow rejects
already recorded games before reaching this sink. Validation failures are
logged without raw Discord payloads and returned to the tracker without
crashing the gateway consumer.

## Configuration and self-host setup

### Rendered game summaries

`/summary` posts a PNG recap of the **latest recorded game across the instance**,
ordered by played date (then game ID). It does not look at unfinished SpellBot
tables or limit the result to the current channel. Use `/summary game:123` for a
Gathering game ID or `/summary game:SB12345` for a recorded SpellBot game. The
`SB` prefix disambiguates the two ID namespaces; lowercase `sb` and `#SB` work too.

The image includes winner artwork, commander/partner portraits, the table's
kills, win condition, date/time (explicitly UTC), duration, turns, and a bounded
notes excerpt. Unknown kills display as `—`, not zero. Draws and missing artwork
have fallback layouts. The message includes a link to the full game and image
alt text; the image is uploaded directly to Discord, not exposed at a public
image URL. Long text is truncated in the image, not in the saved game.

Only active users who have signed into this instance with Discord can invoke it.
DMs are rejected; when `DISCORD_GUILD_ID` is set, other servers are rejected too.
The recap is **public in the invoking channel**, including its notes. Restrict
the command's channels/roles in Discord's integration settings if necessary.
Errors before rendering are private. Rendering is deferred to meet Discord's
three-second acknowledgement deadline, with a server-wide cap of 30 renders
per minute. An acknowledgement failure is not retried to avoid duplicate posts.

Summary uploads use a direct HTTPS interaction-webhook request with a 15-second
request timeout and three-second connection/pool limits, rather than Nostrum's
indefinitely waiting REST queue. Uploads do not follow redirects or retry,
including on rate limits. Logs separately mark rendering, rendered PNG size/time,
and upload start/completion; failures include safe HTTP/Discord codes or timeout
reasons, never interaction tokens or response bodies.

The runtime needs `rsvg-convert` and DejaVu fonts. The Docker image includes
Alpine's `rsvg-convert` and `font-dejavu`; Debian development hosts need
`sudo apt-get install librsvg2-bin fonts-dejavu-core`. Orb setup and CI install
these too. Only HTTPS artwork on `cards.scryfall.io` is fetched, with redirects
disabled, size/time limits and raster-only data embedded in the SVG. Temporary
render files are deleted after conversion. Art failures do not prevent summaries.

Signed-in members can preview the exact PNG at `GET /api/games/:id/summary`.
This endpoint remains session-protected with `private, no-store` caching; it is
also subject to the render cap. No summary request modifies the game.

| Variable | Required | Meaning |
| --- | --- | --- |
| `DISCORD_CLIENT_ID` | yes for member sign-in | Discord application ID. |
| `DISCORD_CLIENT_SECRET` | yes for member sign-in | OAuth2 client secret. |
| `DISCORD_BOT_TOKEN` | yes to enable | Secret bot token. Unset/empty means no Discord process starts. |
| `DISCORD_GUILD_ID` | no | Register `/won` and `/summary` immediately in one server; omit for global commands, which can take up to an hour to appear. Also restricts summary invocation to that server. |
| `DISCORD_SPELLBOT_USER_ID` | no | Trusted SpellBot bot user ID; defaults to production SpellBot (`725510263251402832`). |

1. In the [Discord Developer Portal](https://discord.com/developers/applications),
   create an application and add a bot.
2. On **Bot**, enable **Message Content Intent**. No Guild Members or Presence
   intent is needed.
3. On **OAuth2 → URL Generator**, select `bot` and `applications.commands`.
   Grant **View Channels**, **Send Messages**, and **Attach Files**
   (`permissions=35840`) for public image summaries. Ensure the bot can view the
   specific channel where SpellBot posts games. It does not need Read Message History.
4. Invite the bot with a URL shaped like
   `https://discord.com/oauth2/authorize?client_id=YOUR_APPLICATION_ID&scope=bot%20applications.commands&permissions=35840`.
5. Put the token and optional IDs in `.env`, then restart the container. Never
   paste the token into logs or support messages.
6. Start a SpellBot game and confirm the container logs
   `Discord observed SpellBot game spellbot:SB… with N player(s)`; raw message
   content is never logged. A listed player then runs `/won` in the game's
   channel (or `/won game:SB…` from another channel in the same server), fills
   out the result draft, and clicks **Save game**. The private confirmation
   names the recorded game ID; `/summary` can then share the recap publicly.

What the bot sees during a SpellBot game, per the [SpellBot source](https://github.com/lexicalunit/spellbot/blob/main/src/spellbot/actions/lfg_action.py):
`/lfg` and `/game` are deferred, so the first `MESSAGE_CREATE` is an empty
"thinking" placeholder; SpellBot then sends the waiting/seated game post as an
interaction followup, and answers validation problems with plain text. When an
`/lfg` game fills, SpellBot **edits the existing post** into `Your game is
ready!`, so the report is parsed from a `MESSAGE_UPDATE`. Only that edit (or a
`/game` post that starts fully seated) produces the `observed` log line; the
other messages are ignored silently.

### Troubleshooting

The bot logs each step at `info`, so `docker compose logs the-gathering` shows
how far it got. Read the log from the top of the last start:

| Log line | Meaning / fix |
| --- | --- |
| `Discord bot disabled: DISCORD_BOT_TOKEN is not set` | The token did not reach the container. Check `.env` and that `docker compose up` was re-run after editing it. |
| `Discord bot could not start … Authentication rejected, invalid token` | The token is wrong or was reset in the Developer Portal. The web app keeps running without Discord; fix the token and restart. |
| `Shard websocket closed (errno 4014, …)` repeating, no `READY` | Discord rejected the requested intents. Enable **Message Content Intent** on the **Bot** page. |
| `Discord bot connected as <bot> in 0 guild(s)` | The bot was never invited to the server. Use the invite URL from step 4. |
| `Discord bot connected …` but the bot looks offline in Discord | The bot sets an online presence ("Watching SpellBot games") right after this line. If the member list still shows it offline, the gateway session dropped afterwards; look for `Shard websocket closed` lines below it. |
| `Discord registered /won and /summary in guild …` but commands are missing | The invite lacked the `applications.commands` scope. Re-invite with the URL from step 4 (re-inviting keeps existing permissions). |
| `Discord registered /won and /summary globally` but commands are missing | Global commands can take up to an hour to appear. Set `DISCORD_GUILD_ID` for immediate registration in one server. |
| `Could not register Discord commands: …` | The API error is included; a `403` usually means the `applications.commands` scope is missing. |
| No `Discord observed SpellBot game …` line when a game starts | The line appears only once the post reads **Your game is ready!** (see the message flow above). Otherwise the bot cannot see the channel (grant **View Channels** there), or the message is from a different SpellBot deployment: set `DISCORD_SPELLBOT_USER_ID` to that bot's user ID. Set `LOG_LEVEL=debug` to log why each SpellBot message was ignored. |

A real Discord smoke test was not run in the orb because no throwaway
application/server credentials were available. The test suite uses the scrubbed
upstream-shaped payload and performs no network calls.
