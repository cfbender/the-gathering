# Discord automatic game tracking

This document records the September 2026 research behind the Discord client and
the intended boundary with the games domain.

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
when a game starts. A listed player finishes it with `/won game:SB12345`; the
reply is ephemeral, and a non-player cannot report themselves as winner.

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
        │ cached in memory                 │
        │                                  ▼
Player runs /won game:SB12345       pluggable Sink
        │                                  ▲
        ▼                                  │
membership check + ephemeral reply         │
        └── GameReport (winner set) ───────┘
```

`GameReport` contains `external_id` (`spellbot:SB12345`), `source`
(`discord`), `played_at`, guild/channel IDs, players with Discord ID/display
name/nullable commander, winner Discord IDs, and scrub-safe raw embed data.
Commanders are `nil` because the ready embed does not contain them.

The observed roster cache is in memory, so a restart after game start currently
makes `/won` return a clear “haven't seen that game” error. Completed reports
are persisted by the games sink described below.

## Game tracking

The Discord supervisor uses `TheGathering.Discord.Sink.Games` by default. A
winnerless SpellBot start remains pending in the tracker's in-memory roster
cache rather than being recorded as a draw. When a listed player uses `/won`,
the sink creates or reuses players by Discord ID and records one `games` row
with `source: "discord"`, the SpellBot ID as `external_id`, and seats in the
order SpellBot listed them. The reporting player is the winner and every other
seat is a loss.

If a report supplies a commander, the sink creates or reuses a deck named for
that commander. It leaves `commander_card_id` unset and the color identity empty;
the ready embed currently supplies no commander, and this path does not query
the card catalog.

Repeated reports are idempotent by `{source, external_id}`. A completed replay
replaces the existing game's timestamp, seats, decks, and results, so corrected
seat order or winner data does not create a duplicate. Validation failures are
logged without raw Discord payloads and returned to the tracker without
crashing the gateway consumer.

## Configuration and self-host setup

| Variable | Required | Meaning |
| --- | --- | --- |
| `DISCORD_BOT_TOKEN` | yes to enable | Secret bot token. Unset/empty means no Discord process starts. |
| `DISCORD_GUILD_ID` | no | Register `/won` immediately in one server; omit for a global command, which can take up to an hour to appear. |
| `DISCORD_SPELLBOT_USER_ID` | no | Trusted SpellBot bot user ID; defaults to production SpellBot (`725510263251402832`). |

1. In the [Discord Developer Portal](https://discord.com/developers/applications),
   create an application and add a bot.
2. On **Bot**, enable **Message Content Intent**. No Guild Members or Presence
   intent is needed.
3. On **OAuth2 → URL Generator**, select `bot` and `applications.commands`.
   Grant only **View Channels** (`permissions=1024`). Ensure the bot can view the
   specific channel where SpellBot posts games. This MVP does not backfill, so
   it does not need Read Message History or Send Messages.
4. Invite the bot with a URL shaped like
   `https://discord.com/oauth2/authorize?client_id=YOUR_APPLICATION_ID&scope=bot%20applications.commands&permissions=1024`.
5. Put the token and optional IDs in `.env`, then restart the container. Never
   paste the token into logs or support messages.
6. Start a SpellBot game and confirm the container logs receipt of
   `spellbot:SB…` without raw message content. A listed player then runs `/won`
   with that ID and should receive an ephemeral confirmation.

A real Discord smoke test was not run in the orb because no throwaway
application/server credentials were available. The test suite uses the scrubbed
upstream-shaped payload and performs no network calls.
