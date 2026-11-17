# Discord integration

The Gathering uses one Discord application for two things:

- **Sign-in.** Members sign in with Discord OAuth. Only the first administrator has a password.
- **An optional bot.** It gathers players for webcam tables with `/newgame`, picks up SpellBot
  games so members can log their results with `/log`, and posts game recaps with `/summary`.

The environment variables are listed in the installation guide's
[configuration reference](installation.md#configuration-reference).

## Set up

### Sign-in

Follow [Let members sign in (Discord)](installation.md#6-let-members-sign-in-discord). How sign-in
behaves after that:

- A Discord account the app has not seen before becomes a member only while **Admin → Settings →
  Open registration** is on, or when the member arrives through a playgroup invite link. Linked
  members can sign in while registration is closed; disabled accounts cannot.
- Sign-in requests the `identify email` scopes. Signing in links the Discord ID to the account
  and to its player.
- A rejected sign-in returns to `/login` with a one-time message. A member turned away while
  registration was closed must click **Continue with Discord** again once it opens; reloading
  the old page does not retry.
- The server logs every rejected callback: `Discord sign-in rejected an unknown account because
  registration is closed` at `info`, and `Discord sign-in failed: …` at `warning` for credential,
  state, or redirect problems.
- Signing in with Discord again also unlocks sensitive admin actions for ten minutes.

### Bot

1. In the [Discord Developer Portal](https://discord.com/developers/applications), open the same
   application. On **Bot**, reset the token and copy it into `DISCORD_BOT_TOKEN`. It is a
   separate secret from `DISCORD_CLIENT_SECRET`; never paste it into logs or support messages.
2. On **Bot**, enable **Message Content Intent**. The bot needs no Server Members or Presence
   intent.
3. Invite the bot with the URL in the installation guide. It requests the `bot` and
   `applications.commands` scopes and these permissions (`326686002176`):

   | Permission | Used for |
   | --- | --- |
   | View Channels | Seeing SpellBot posts and the channels commands run in |
   | Send Messages, Embed Links | Queue posts, pings, and announcements |
   | Attach Files | `/summary` images |
   | Create Public Threads, Send Messages in Threads | `/newgame` threads |
   | Manage Threads | Renaming threads and `/close` |
   | Manage Roles | `/alerts` |

   The bot must be able to view each channel where SpellBot posts games. It does not need Read
   Message History. A bot invited before game threads existed keeps its old permissions:
   invite it again with the current URL, or grant the missing permissions to its role.
4. Set `DISCORD_GUILD_ID` to your server's ID so commands appear immediately. Without it the
   commands are global, can take up to an hour to appear, and work in every server the bot is
   in. With it, the bot refuses its commands in other servers.
5. Restart the server. The log shows `Discord bot connected as <name> in N guild(s)` and then
   `Discord registered /log, /summary, /newgame, /close, and /alerts in guild …`.
   Registration also removes the old `/won` command and leaves other commands alone.

To serve several Discord servers from one bot, leave `DISCORD_GUILD_ID` unset and set each
server's ID on its playgroup (**Admin → Playgroups**). Games from a server that no playgroup
claims go to the default playgroup.

## Webcam-table queues with `/newgame`

`/newgame` posts a public queue for a webcam table. Any member of the server can run it or join,
without an app account. Every option is optional:

| Option | Meaning |
| --- | --- |
| `start` | When to start: `8pm`, `20:30`, `930pm`, `in 45m`, `in 2h`, `tomorrow 7pm`, or a Discord timestamp such as `<t:1790000000:F>`. Omit it to start as soon as the minimum joins. |
| `min_players` | Minimum roster, 2–10. Default 3. |
| `title` | Up to 100 characters. Default `Commander game`. |
| `format` | Free text, up to 100 characters. Default `Commander`. |

### Buttons

The queue shows the title, start time, minimum, format, roster, and maybe list.

- **Join** adds you to the roster (at most ten players). The host is not added automatically and
  must click Join to play.
- **Maybe** puts you on a separate list of up to ten that does not count toward the minimum.
  Join and Maybe move you between the lists.
- **Leave** removes you from both lists.
- **Change time** opens a form that takes the same syntax as `start`. Leave it blank to start as
  soon as the minimum joins (at once, if it already has).
- **Cancel** cancels the game.

Only the host or a member with Discord's **Administrator** permission (the server owner has it)
can use Change time and Cancel. The app's admin role and Discord's Manage Server permission do
not count. Every reply to a button is private. The buttons are disabled once the game starts,
expires, or is cancelled.

### Start times

Clock times use `DISCORD_DEFAULT_TIMEZONE` (default `America/New_York`). A bare time means today
if it is still ahead, otherwise tomorrow. `tomorrow` means the next calendar day, not 24 hours
later. `in 45m` and `in 2h` count elapsed time. Times in the past are rejected, and so are clock
times that a daylight-saving change skips or repeats; use a Discord timestamp for those.

### Starting, expiring, and maybe pings

The bot checks queues at boot and every five seconds after that.

- A queue without a start time starts as soon as the minimum joins and otherwise waits until it
  fills or is cancelled.
- At its start time, a scheduled queue starts if the minimum has joined.
- If it is short and nobody said Maybe, it expires.
- If it is short and someone said Maybe, the bot pings only the maybe list and keeps the queue
  open for 15 more minutes. It starts as soon as the minimum joins, or expires after that.
  Changing the time arms the ping again for the new start.

When a game starts, the bot pings the roster with a link to a new webcam table
(`<THE_GATHERING_PUBLIC_URL>/table/<id>`). Players sign in with Discord to enter it; the normal
registration and disabled-account rules apply. The roster coordinates players; it does not
reserve seats or restrict who can enter.

Queues live in the database, so they survive restarts. Queues that came due while the server was
down are settled at the next boot. A failed ping or queue edit is retried on the next check, and
a game can only start once. If the bot's first reply to `/newgame` fails, the queue is cancelled;
run `/newgame` again.

### Threads

The bot starts a thread on its `/newgame` reply and moves the queue into it. The reply becomes a
"sign up in the thread below" pointer. The thread is named after the title and the game's date
in `DISCORD_DEFAULT_TIMEZONE`, for example `Friday pod · Sep 24` (games without a start time use
the day they were posted). It is renamed when Change time moves the game to another day, and it
archives after seven days without messages.

- The host is added to the thread, and so is anyone who clicks Join or Maybe. Leave does not
  remove anyone.
- The ready and maybe pings are posted in the thread, which also adds anyone they mention.
- Running `/newgame` inside a game thread posts the new queue in that thread and renames it.
- **`/close`** inside a game thread archives it and cancels its open games. Only the host of the
  thread's newest game or a Discord Administrator can run it. A new message reopens the thread.

Threads are optional. Without the thread permissions, or in a channel that cannot hold threads,
the reply itself shows the queue and pings go to the channel. A ping the thread refuses (for
example, because it was deleted) also goes to the channel.

### Game alerts role

Discord shows a thread in someone's channel list only after they join it. To bring interested
members in automatically:

1. Create a role such as "Game alerts". Turn on **Allow anyone to @mention this role**, or give
   the bot **Mention @everyone, @here, and All Roles**.
2. Move the bot's own role above it in the server's role list.
3. Set `DISCORD_NEWGAME_ROLE_ID` to the role's ID and restart.

The bot then mentions the role in each new game's thread, which adds the role's members to it.
Discord does this only for roles with fewer than 100 members. Members run **`/alerts`** to give
themselves the role and again to remove it; the reply is private.

## SpellBot tracking

[SpellBot](https://github.com/lexicalunit/spellbot) seats players and starts games, but it never
learns the result. The bot fills that gap:

1. When SpellBot edits its queue post into **Your game is ready!**, the bot reads the roster,
   start time, and SpellBot game ID and logs `Discord observed SpellBot game spellbot:SB… with N
   player(s)`. It accepts only posts from `DISCORD_SPELLBOT_USER_ID` (default: the public
   SpellBot) and only games of two to six players. It never logs message text.
2. The game waits in the database without a winner. Seeing the same game again updates it
   instead of adding a second copy. A waiting game is dropped 30 days after it was last seen.
3. Any member of that server runs **`/log`** in the game's channel. `/log` picks the newest
   unlogged game in that channel; `/log game:SB12345` picks one from any channel in the same
   server. `winner:@player` preselects a winner, who must be in the game. Replies are private.
4. **Open game log** opens `/games/new` with the roster, date, estimated duration, and winner
   filled in. Sign in with the same Discord account (administrators can open anyone's link).
   Fill in decks, the winner or a draw, and anything else, then click **Save game**.

Nothing is created until **Save game**. The link expires after one hour and stops working if
the game changes or is logged by someone else; run `/log` again for a new one. Saving matches
players by their Discord IDs, records the game with its SpellBot ID, and removes it from the
waiting list in one step. The member who saves it is its creator. To change a logged game, edit
it in the web app.

Administrators see waiting games under **Admin → Discord**, where they can pick a
winner from the roster or discard the game.

## Other commands

**`/summary`** posts a PNG recap of a recorded game in the channel, publicly, including its notes.
Without an option it shows the newest game of the server's playgroup. `game:123` picks a
Gathering game ID and `game:SB12345` a logged SpellBot game. Only members who have signed in to
the app with Discord and are not disabled can run it. Errors before rendering are private.
Rendering is limited to 30 images a minute across the server, and needs the DejaVu fonts (the
Docker image includes them). Commander art comes only from Scryfall's image CDN; a summary still
renders when art fails. Signed-in members can preview the same image at
`GET /api/games/:id/summary`. To keep recaps out of some channels, restrict the command in
Discord's **Server Settings → Integrations**.

## Troubleshooting

Sign-in problems are covered in the installation guide's
[troubleshooting table](installation.md#troubleshooting). For the bot, read the server log from
the last start (`docker compose logs the-gathering`, or **Admin → Server logs**):

| Log line or symptom | Cause and fix |
| --- | --- |
| `Discord bot disabled: DISCORD_BOT_TOKEN is not set` | The token did not reach the server. Check `.env` and recreate the container with `docker compose up -d`. |
| `Discord bot could not start; … close code 4004 …` | The token is wrong or was reset. The web app keeps running; fix the token and restart. |
| `Discord bot could not start; … close code 4014 …` | Discord refused the intents. Enable **Message Content Intent** on the **Bot** page and restart. |
| `Discord bot connected as <name> in 0 guild(s)` | The bot is not in any server. Invite it (step 3). |
| Bot shows offline after `Discord bot connected …` | The gateway session dropped later. Look for `Discord gateway error` or `Discord gateway connection ended` below it. |
| `Could not register Discord commands: HTTP 403, …` | The invite lacked the `applications.commands` scope. Invite the bot again; it keeps its permissions. |
| `… registered … globally; new commands can take up to an hour to appear` | Wait, or set `DISCORD_GUILD_ID` for immediate registration. |
| `Discord newgame N thread could not be opened: HTTP 403, Discord code 50013` | The bot lacks the thread permissions in that channel. The queue still works without a thread. |
| `/close` says the thread could not be closed | Give the bot **Manage Threads**, then run `/close` again. |
| `/alerts` says alerts could not be changed | Give the bot **Manage Roles** and move its role above the alerts role. |
| No `Discord observed SpellBot game …` line | The line appears only once SpellBot's post reads **Your game is ready!**. Otherwise the bot cannot view that channel, or the game came from another SpellBot: set `DISCORD_SPELLBOT_USER_ID` to its user ID. `LOG_LEVEL=debug` logs why each SpellBot message was ignored. |
| `Could not record Discord game spellbot:SB…: …` | The game could not be staged, for example because it had more than six players. |

## Why it works this way

SpellBot only starts games: it has no result API or webhook to subscribe to, so the bot reads
SpellBot's ready embed from the gateway. Embeds count as message content, which is why the bot
needs Discord's
[Message Content intent](https://docs.discord.com/developers/events/gateway#message-content-intent).
Convoke, the webcam table SpellBot often links to, offers no public hook for game results either,
so the result is entered in the app through `/log`.

For contributors: the bot lives in `rust/crates/the-gathering/src/discord/` (`mod.rs` describes
the modules). Tests drive it through a fake `DiscordApi` and need no network. The SpellBot parser
test uses `rust/crates/the-gathering/tests/fixtures/discord/spellbot_game_ready.json`, an
upstream-shaped embed with invented IDs and names.
