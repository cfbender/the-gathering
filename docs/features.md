# Features

How each part of The Gathering behaves. For installing and configuring a server, see
[Installation](installation.md); for administrator tools, see [Administration](administration.md).

## Webcam table

**Games → Play** opens a temporary room for up to ten signed-in members who share their
board cameras through a WebRTC SFU built into the server (each browser uploads one simulcast
stream and receives every other board at the resolution it is drawn) and record the result
when the game ends. The SFU listens on the UDP range `WEBRTC_SFU_PORT_RANGE`; forward it to the
host and set `WEBRTC_SFU_PUBLIC_IP`, or set `WEBRTC_SFU_RELAY_ONLY=true` to carry media over the
configured TURN servers instead (see `docs/webcam-table.md`).

**Reveal hand to** privately sends your camera to one chosen player; everyone else receives no
video until you end the reveal or the target leaves.

Clicking a card on any board identifies it with the recognizer bundle published from
[Oracle](https://github.com/cfbender/oracle) (see its README, "Shipping"): the server downloads the newest `scanner-bundle-*` release of this
repository at boot and every six hours (`CARDID_BUNDLE_SOURCE`), serves `DATA_DIR/cardid/current/*`
at `/api/cardid/*`, and the browser runs the models itself. A recognized card opens with its rules text and
rulings (from the local catalog) and lands in that board's card tray for every seat.
Without a published bundle the table still works and offers the player's commanders as
suggestions instead.

When a seated player's chosen deck links to a list, every seat loads it
and warms its card images, **Decks → View decklist** shows your own list, and the recognizer
favours cards in the clicked board owner's list (see Oracle's README, "Deck-list prior").
Opponents' lists are not shown in the UI but are visible in the browser's network tab.

**Use virtual board** in your seat menu replaces your camera with a virtual tabletop dealt
from your deck's linked list; other players see the board, with your hand and library as counts.

**Trackers** over your own camera keep free-form 0–100 counters ("Lands", "Creatures in graveyard")
and a combat-math list: enter each anthem or combat buff with its conditions (attacking, blocking,
flying, tokens, nontoken, or any creature type) and the overlay shows the combined bonus for every
kind of creature ("Attacking token creatures +4/+4 · vigilance"). Each counter and the buff list has
its own "Show to table" switch; shared ones appear on your seat for everyone and shared counter
changes are logged, while private ones stay in your browser.

**Rules** asks an AI judge for a ruling. The server sends the question, the earlier turns of the
conversation, and (when the player ticks the box) the commanders and identified cards at the table to a model on
[OpenRouter](https://openrouter.ai), which answers with tools that search and read the Magic
Comprehensive Rules and look cards up in the local catalog (current Oracle text plus Scryfall's
rulings). Answers cite rule numbers, show the cited rules' text, and preview named cards on hover.
The conversation stays in the browser tab. An administrator enables it under **Admin → Settings →
AI rulings** with an OpenRouter API key (stored encrypted) and any model ID that supports tool
calling; saving checks both with OpenRouter. Every question's cost, as OpenRouter reports it, is
recorded per member: set **Monthly limit per member (USD)** to cap each member's spending per
calendar month (UTC); administrators are not limited. This month's spending per member is on the same page. Questions are also
limited to 30 per member per 10 minutes. Requests mark prompt-cache breakpoints and keep each
conversation on one provider, so models that support prompt caching bill the repeated system prompt,
tools, and earlier lookups at their cheaper cached rate; the cached token counts are recorded with
each question's cost.

The server downloads the Comprehensive Rules from the text file linked on
[magic.wizards.com/en/rules](https://magic.wizards.com/en/rules) shortly after boot, then checks that
page daily and installs each new release (`RULES_SYNC_ENABLED=false` turns the checks off). **Admin →
Catalog → Comprehensive Rules** shows the installed release and checks now.

Design notes are in [Webcam table design](webcam-table.md); to improve recognition or contribute training data,
see Oracle's [CONTRIBUTING.md](https://github.com/cfbender/oracle/blob/main/CONTRIBUTING.md).

### Scheduling games from Discord

Use `/newgame [start] [min_players] [title] [format]` to post a public Join/Maybe/Leave
queue. The default minimum is three (configurable from two to ten). Without
`start`, it opens a webcam-table link as soon as the minimum joins; scheduled
queues start or expire at the requested time. Maybe doesn't count toward the
minimum, but if a scheduled game is short at its start time the bot pings the
maybe list and waits 15 minutes for them to join. The host or a Discord Administrator
can Change time or Cancel. The queue and its buttons live in a thread on the
`/newgame` reply, where the ready and maybe pings land too; the host or an Administrator runs `/close` in it to
close it. See [Discord integration](discord-integration.md#webcam-table-queues-with-newgame)
for time syntax, permissions, and restart/delivery behavior.

## Tabletop

**Games → Tabletop** (`/tabletop`) turns one device into a life counter for a table where
everyone plays in person. Pick a format, two to six players, how they sit around the device,
and starting life; then each player taps their seat to sit down as a playgroup member (with one
of their decks) or as a guest. Tap the left half of a seat to lose life and the right half to
gain it; hold to change it by ten. Each seat's counters button tracks poison and conceding, and
while it is open every opponent's seat turns into a commander damage counter: tap an opposing
commander to add the damage it dealt (which also comes off life). Two players can also share
one edge of the device (**Same side**), so a sideways phone reads like an ordinary landscape
life counter. The hub in the middle rolls for the first player, tracks turns and the game clock,
and ends the game.

A game is recorded in game history only when every seat is a member and the format is one
history records; a game with any guest (or empty seat), or in Standard, Pauper, or Canadian
Highlander, just ends. The game in progress is saved in the browser, so a reload or reopening
the installed app picks it up again, and the screen stays awake while the board is open.

## Game history

### Kill tracking

When logging or editing a game, use each player's **Kills** dropdown to select who they
killed. **Total kills** includes those selections; enter a larger total (or only a total)
when some names are unknown. When logging a game, leaving **Total kills** blank records
`0`; when editing an existing game, leaving it blank preserves or clears the count to
unknown.
Game details show named victims and any unknown remainder. A victim can have only one killer.
Totals, including CSV corrections, cannot be lower than the number of named victims.
The live table's **Record game result** modal also offers optional kills and MVP cards for
each player, saved with either **Record result** or **Record and rematch**.
Player rivalries show who you killed most, who killed you most, and directional kill counts
for each opponent in the selected date range. These count named victims only; number-only
kills stay unattributed, and win/loss rivalry rankings are unchanged.
On player profiles, rivalry cards and head-to-head counts open the matching games, retaining
the date range. Commander rivalry counts also link to games with that opposing commander;
mirror matches can contribute multiple seats to a count, but the list shows each game once.

On upgrade, existing games with a sole winner who recorded exactly one kill per opponent
are attributed to that winner, unless existing attribution contradicts it. Other totals
remain unchanged and unattributed. This inference runs once on historical games, not on
new count-only entries. Portable exports retain both totals and attribution.


## Playgroups

Every player, deck, and game belongs to one playgroup. Existing servers get a single
playgroup named **Main** holding everything recorded so far, with every existing user as
a member. Playgroups are private: a member only sees the playgroups they belong to, and can
belong to several. The switcher next to the logo picks the playgroup being viewed, saved
to the account; games logged, imported, or played at a table go into the viewed playgroup.
A user who belongs to no playgroup sees an empty state until an administrator adds them
or they accept an invite link.

Administrators manage playgroups under **Administration → Playgroups** (sudo required):
create (the creator becomes the first member), rename, add and remove members, and
delete. Administrators see every playgroup on that page but still only view the ones
they belong to. Deleting removes the playgroup's players and memberships, so it is only
allowed once it has no games and no decks, and the last playgroup can't be deleted.

A game recorded in the wrong playgroup can be moved from its page (administrators, sudo
required) into another playgroup the administrator belongs to. Each seat is re-pointed to
that playgroup's player with the same account, Discord identity, or name (created when
missing) and to their deck of the same name (copied when missing); the original players
and decks stay where they are. The move is refused when a seat's name matches a player
there who is linked to a different account or Discord identity.

A playgroup can claim a Discord server ID so the bot records that server's games there;
servers no other playgroup claims record into the oldest playgroup. To serve several
Discord servers from one bot, leave `DISCORD_GUILD_ID` unset (commands register globally)
and set each server's ID on its playgroup. `DISCORD_NEWGAME_ROLE_ID` holds a single role
ID, so `/newgame` alerts only work in one server.

## Invite members with a playgroup link

Each playgroup card under **Administration → Playgroups** has a **Shareable invite link**
for Discord sign-in. Copy and save it before leaving the page: only its hash is stored, so
the secret cannot be retrieved later. Anyone with the link joins that playgroup, as a new
member even with **Open registration** off, or as an additional playgroup when they already
have an account (a signed-in user is added and switched immediately). Links do not expire
or limit uses. Discord-identified accounts get a player in the playgroup automatically;
password accounts are linked to a player by an administrator.

**Rotate link** generates a replacement and immediately revokes the old link's
registration permission, including OAuth sign-ups that have not completed yet.
Already registered users keep their access; disabled accounts remain blocked.
Normal open registration (which joins the oldest playgroup) and the initial password
administrator setup are unchanged.
Creating or rotating links requires administrator sudo authentication. The secret
uses a URL fragment (not a logged request path or query), then a signed session
carries its digest through Discord OAuth. Treat the full link as a credential.

## Deck-list links

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

## Appearance

**Settings → Appearance** picks a color palette (Claret, Nord, Catppuccin, Tokyo Night,
Gruvbox, Everforest, Kanagawa, Night Owl, Dracula, Rosé Pine, Solarized, or Monochrome) and a surface style (Liquid glass or
Classic). Both are saved on the user account through `PATCH /api/session/appearance`
(`{"palette": "...", "theme_style": "glass" | "classic"}`), returned in user
JSON, and rendered onto `<html>` by the SPA shell so the first paint already matches on
any device. Light/dark mode stays a per-device preference on the navigation toggle.

## Deck chooser

**Decks → Choose a deck** picks from the signed-in user's linked player's active
decks. Selection uses ManaVault's weighting: hours since last played, multiplied by
current skips plus one, divided by recorded plays plus one. Never-played decks get a
30-day boost beyond the stalest played deck. Plays and last-played dates come from
game seats; only the current skip count and per-deck chooser inclusion setting are
stored. Skipping records the skip before rerolling, while choosing clears that deck's
skips.

Members can replace that weighting with their own Lua script under **Choose a deck →
Customize**. A script defines `weight(deck, ctx)` and sees each deck's plays, skips,
results, recent games, and colors, plus the local time, games played today and tonight,
and the member's recent history. The editor previews every deck's chance under the draft
before saving. Scripts run sandboxed on the server with memory, instruction, and time
limits, and a script that fails falls back to the built-in weights for that pick. See
[Deck chooser scripts](deck-chooser-scripts.md) for the contract, every field,
and the examples.

Hosted decks are not listed separately: **Sync hosted decks** (on your player profile
and the chooser, shown once any deck host is configured) calls `POST
/api/session/remote-decks/sync`, which folds only decks explicitly marked as Commander
by Moxfield, Archidekt, or ManaVault into your player's deck list. Other formats and
decks with missing or unknown formats are skipped, even if they have commanders.
Previously imported decks are not removed, and individual deck-link imports are
unaffected. A remote deck matches the local deck
with the same deck-list URL or name, then an unlinked local deck with the same
commander pair; otherwise it is added as a new deck using the host's details.
Existing decks keep their local names, commanders, selected art, colors, archive
status, and chooser inclusion. Sync only adds a missing deck-list link; it never
re-points an existing link. The response's `updated` count includes all matched
existing decks, even when nothing changed. Hosts that fail to list are skipped and
reported in the response's `errors`.

