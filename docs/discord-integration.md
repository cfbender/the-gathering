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
when a game starts. Any server member finishes it with `/log`, which defaults to
the most recently started game the bot has seen in that channel, or
`/log game:SB12345` to pick a specific one. The ephemeral reply opens the regular
web game form; optionally mention `winner:@player` to preselect the winner.

This fills the exact gap SpellBot leaves while avoiding screen-scraping its HTML
or depending on private Convoke APIs. It also keeps the integration usable for
SpellBot games launched on services other than Convoke.

**Fallback:** pursue a direct Convoke partnership. Its completion event has the
best data (winner, commanders/decks and richer outcomes), and could feed the same
`GameReport`/sink boundary. Until such access exists, a message-history backfill
can improve missed-roster recovery but cannot recover winners.

Reactions were rejected for the initial UX: they are easy to place on the wrong
message, add reaction intent/permission complexity, and still require a policy
for who may choose another player's winner. `/log` is explicit and attributable.

## Implemented flow

```text
SpellBot edits ready embed
        │
        ▼
Discord gateway (MESSAGE_UPDATE / MESSAGE_CREATE)
        │ author ID + embed contract validation
        ▼
GameReport (winner_discord_ids: [])
        │ staged in SQLite
        ▼
Member runs /log [winner:@player] [game:SB12345]
        │
        ▼
server check + private web link
        │ sign in, complete game form
        ▼
Save → atomic game creation + consume pending report
```

`GameReport` contains `external_id` (`spellbot:SB12345`), `source`
(`discord`), `played_at`, guild/channel IDs, players with Discord ID/display
name/nullable commander, winner Discord IDs, and scrub-safe raw embed data.
Commanders are `nil` because the ready embed does not contain them.

### Recording a result with `/log`

Any member of the game's server can run `/log`, even if they did not play.
Use `/log winner:@player` to preselect a roster member, and optionally add
`game:SB12345`. The reply, including errors, is visible only to the command
runner. **Open game log** opens `/games/new` with a private draft ID. Sign in
with the same Discord account; administrators can also access drafts. The link
does not bypass registration or disabled-account restrictions.

The web form prefills the roster, date, estimated duration, and optional winner.
Without a winner mention, choose a winner or draw before saving. Edit or clear
the duration when reporting an older game. Use the normal web controls for
**decks/commanders per player, win condition, turns, kills, MVP cards, and notes**.
The SpellBot roster is fixed, but seats can be reordered. Enter `0` kills for
none or leave blank for unknown.

Card search tolerates case, accents, omitted apostrophes and commas: `Jeskas
will`, `sephiroth fabled soldier`, and `Terra magical adept` find the catalog
names. Leading names such as `Bello` rank ahead of broad substring matches.
Choose from the web search results rather than typing names into Discord modals.

Opening the link creates no players, decks, or games. Nothing is recorded until
**Save game**. Drafts persist across restarts for one hour and are bound to the
runner and original roster/start time. Changed or expired drafts must be
reopened with `/log`. Saving resolves players by the staged Discord identities,
validates deck ownership, creates the game, and consumes the pending report in
one transaction; competing drafts cannot overwrite it. Failed validation rolls
back new players and decks as well. The authenticated submitter is the creator.
Already recorded games must be edited in the web app. To remove a test game,
open its game-detail page and choose **Delete game**, then confirm. Admins,
the creator, and linked participants have the same deletion permission as Edit.
Deletion removes the game and seats, not the players or decks, and cannot be undone.

Winnerless reports are staged in SQLite, so `/log` continues to work after an
application or Tracker restart. Re-observing the same SpellBot external ID
updates its staged timestamp, roster, commander names, and scrub-safe normalized
data rather than creating a duplicate. Without a `game` option, `/log` queries
the latest staged report in the invoking channel that does not already have a
recorded Discord game with the same external ID. This skips games whose winners
were previously recorded but which a later SpellBot edit re-staged, and reports
that no game is available when every staged game in the channel is already
recorded. An explicit ID may select a game from another channel in the same
server. The authenticated API is `GET`/`POST /api/discord/result-drafts/:id`.

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
storage while leaving a month for `/log` or administrator recovery. The staged
data is normalized; raw Discord payloads are never stored in full or logged.

## Game tracking

The Discord supervisor uses `TheGathering.Discord.Sink.Games` by default. A
winnerless SpellBot start remains in durable staging rather than being recorded
as a draw. `/log` hands off to `WebGameDraft` and `SaveWebGame`, which create or
reuse players by Discord ID and record `source: "discord"` with the SpellBot ID
as `external_id`. The submitted seat order, results, decks, win condition,
turns, duration, notes, kills, and MVP cards are saved through the games context.
The ready embed itself supplies no commander. Legacy reports that supply only
a commander name retain their name-only deck fallback in the games sink.

Repeated sink reports are idempotent by `{source, external_id}`. A completed replay
replaces the existing game's timestamp, seats, decks, and results, so corrected
seat order or winner data does not create a duplicate. The `/log` flow rejects
already recorded games before reaching this sink. Validation failures are
logged without raw Discord payloads and returned to the tracker without
crashing the gateway consumer.

## Configuration and self-host setup

### Webcam-table queues with `/newgame`

`/newgame` is guild-only and accepts four **optional** options:

| Option | Type | Meaning |
| --- | --- | --- |
| `start` | string, max 100 characters | `8pm`, `20:30`, `in 45m`, `in 2h`, `tomorrow 7pm`, or Discord `<t:unix>` / `<t:unix:F>` / `<t:unix:R>`. Omit to start when filled. |
| `min_players` | integer 2–10 | Minimum roster size; defaults to 3. |
| `title` | string, max 100 characters | Defaults to `Commander game`. |
| `format` | string, max 100 characters | Free text; defaults to `Commander`. |

The public embed shows the title, start time, minimum, format and roster. Join
and Leave update it in place and privately confirm the action. Repeated clicks
do not duplicate players; the roster caps at ten. **The host must click Join to
play**, just like everyone else. Cancel is a button, not a slash subcommand;
only the host or a Discord **Administrator** (including the guild owner) can
cancel. The app's admin role and Discord Manage Guild alone do not grant this
permission. Guild roles come from Nostrum's cache because version 0.10 drops
the interaction's member permission field. All buttons are bound to the original
guild, channel and message, and disabled once started, cancelled or expired.

Any member of the server can queue, without first linking an app account. To
enter the actual table, players must sign in with Discord; normal registration
and disabled-account restrictions still apply. The roster is coordination, not
a seat reservation or room access list. The lobby URL uses the Phoenix endpoint's
`PHX_HOST`, `PHX_SCHEME`, and `PHX_URL_PORT`; it identifies a UUID room without
creating presence. It appears in active tables only after someone enters.

Bare clock times use `DISCORD_DEFAULT_TIMEZONE` (IANA, default
`America/New_York`), choosing today if still future, otherwise tomorrow.
`tomorrow` means the next local calendar day, not 24 elapsed hours. Invalid or
past explicit timestamps are rejected privately. Nonexistent or ambiguous DST
clock times are rejected with a request for a Discord timestamp. The `tz`
dependency supplies bundled IANA rules; update it with app releases (no runtime
timezone downloads). Discord `<t:unix:F>` and `<t:unix:R>` echoes show the parsed
time in each viewer's timezone. `in Nm` / `in Nh` mean elapsed time.

The supervised scheduler scans SQLite on boot and every five seconds, in batches
of 100; Discord request/rate-limit delays can extend that interval. At the deadline
the existing roster starts if its minimum is met; otherwise it expires. A click
at or after the deadline cannot rescue an underfilled queue. Unscheduled queues
wait indefinitely until filled or cancelled. Overdue queues are processed on
restart, even after a long outage. Records and rosters are retained, not deleted.
The status transition is an atomic `UPDATE ... WHERE status = 'open'`; the room
UUID cannot be replaced by a second start. Roster changes and message edits are
serialized by the scheduler; API calls happen outside database transactions.

A ready message mentions only joined players and posts the lobby link. Failed
notifications stay pending in SQLite and retry; the announcement ID is saved
before updating the original embed so an edit failure does not repeat the ping.
A stable Discord nonce also deduplicates short-window announcement retries.
Discord does **not** offer permanent nonce deduplication: a crash after Discord
accepts a post but before its ID is saved, followed by a long outage, can repeat
the announcement, but never creates a second room. Initial slash-response
placeholder failures are not retried automatically; rerun `/newgame`. The message
ID is saved before exposing buttons, so the first actionable queue edit can be
retried after a restart. A crash before saving that ID leaves only a placeholder
and requires a new command. Deleted messages or revoked channel permissions leave pending work
and warning logs until access is restored; records are preserved for diagnosis.

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
| `DISCORD_GUILD_ID` | no | Register `/log`, `/summary`, and `/newgame` immediately in one server; omit for global commands, which can take up to an hour to appear. Also restricts invocation to that server. |
| `DISCORD_DEFAULT_TIMEZONE` | no | IANA timezone for `/newgame` clock times; defaults to `America/New_York`. |
| `DISCORD_SPELLBOT_USER_ID` | no | Trusted SpellBot bot user ID; defaults to production SpellBot (`725510263251402832`). |

1. In the [Discord Developer Portal](https://discord.com/developers/applications),
   create an application and add a bot.
2. On **Bot**, enable **Message Content Intent**. No Guild Members or Presence
   intent is needed.
3. On **OAuth2 → URL Generator**, select `bot` and `applications.commands`.
   Grant **View Channels**, **Send Messages**, **Embed Links**, and **Attach Files**
   (`permissions=52224`) for queues and public image summaries. Ensure the bot can view the
   specific channel where SpellBot posts games. It does not need Read Message History.
4. Invite the bot with a URL shaped like
   `https://discord.com/oauth2/authorize?client_id=YOUR_APPLICATION_ID&scope=bot%20applications.commands&permissions=52224`.
5. Put the token and optional IDs in `.env`, then restart the container. Never
   paste the token into logs or support messages.
6. Start a SpellBot game and confirm the container logs
   `Discord observed SpellBot game spellbot:SB… with N player(s)`; raw message
   content is never logged. Any member then runs `/log` in the game's channel
   (or `/log game:SB…` from another channel in the same server), opens the private
   link, completes the web form, and clicks **Save game**. `/summary` can then
   share the recap publicly.

Registration replaces `/won` with `/log` in the configured command scope while
preserving unrelated commands. Existing modal drafts remain supported briefly
for users who opened them before upgrading. Set `PHX_HOST`, `PHX_SCHEME`, and
`PHX_URL_PORT` to the public app address so the generated links work externally.

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
| `Discord bot connected …` but the bot looks offline in Discord | The bot sets an online presence ("Watching the battlefield") right after this line. If the member list still shows it offline, the gateway session dropped afterwards; look for `Shard websocket closed` lines below it. |
| `Discord registered /log, /summary, and /newgame in guild …` but commands are missing | The invite lacked the `applications.commands` scope. Re-invite with the URL from step 4 (re-inviting keeps existing permissions). |
| `Discord registered /log, /summary, and /newgame globally` but commands are missing | Global commands can take up to an hour to appear. Set `DISCORD_GUILD_ID` for immediate registration in one server. |
| `Could not register Discord commands: …` | The API error is included; a `403` usually means the `applications.commands` scope is missing. |
| No `Discord observed SpellBot game …` line when a game starts | The line appears only once the post reads **Your game is ready!** (see the message flow above). Otherwise the bot cannot see the channel (grant **View Channels** there), or the message is from a different SpellBot deployment: set `DISCORD_SPELLBOT_USER_ID` to that bot's user ID. Set `LOG_LEVEL=debug` to log why each SpellBot message was ignored. |

A real Discord smoke test was not run in the orb because no throwaway
application/server credentials were available. The test suite uses the scrubbed
upstream-shaped payload and performs no network calls.
