# Features

How each part of The Gathering behaves for the people using it. For installing and configuring a
server, see [Installation](installation.md); for administrator tools, see
[Administration](administration.md).

## Webcam table

**Games → Play** opens a room for up to ten signed-in members. Each player points a camera at
their board and everyone sees every board. When the game ends, **Record game result** saves it to
game history. Video runs through a media server built into The Gathering; players outside your
network need the setup in [Installation](installation.md#8-webcam-video-from-outside-your-network).
How the table works inside is in [Webcam table design](webcam-table.md).

Your seat menu has:

- **Reveal hand…** sends your camera to one chosen player only. Everyone else sees no video from
  you until you end the reveal or that player leaves.
- **Use virtual board** replaces your camera with a virtual tabletop dealt from your deck's linked
  list. Other players see your board, with your hand and library as counts.

**Trackers** over your own camera keep free-form counters from 0 to 100 ("Lands", "Creatures in
graveyard") and a combat-math list. Enter each anthem or combat buff with its conditions
(attacking, blocking, flying, tokens, nontoken, or any creature type), and the overlay shows the
combined bonus for each kind of creature ("Attacking token creatures +4/+4 · vigilance"). Each
counter and the buff list has its own **Show to table** switch. Shared trackers appear on your seat
for everyone, and changes to shared counters go into the table log. Private ones stay in your
browser.

### Identifying cards

Click a card on any board to identify it. The card opens with its rules text and rulings and lands
in that board's card tray for every seat. Recognition runs in each browser with a model bundle the
server downloads (see [Installation](installation.md#9-optional-features)). Without a bundle the
table still works and offers the player's commanders as suggestions instead.

When a seated player's deck links to a Moxfield, Archidekt, or ManaVault list, every seat loads
that list, and recognition favours cards from the list of the board that was clicked. **View
decklist** in the table's **Decks** tab shows your own list. Opponents' lists are not shown in the
app, but anyone can read them in the browser's network tab.

### AI rulings

The table's **Rules** tab asks an AI judge. It searches the Magic Comprehensive Rules and looks up
cards in the local catalog (current Oracle text and Scryfall rulings), then answers with the rule
numbers it used. Cited rules show their text, and named cards preview on hover. Tick the box to
include the cards in play, and the question also sends the table's commanders and identified
cards. The conversation stays in your browser tab; the last few turns go with each question. AI
can be wrong, so check the cited rules when it matters.

The judge is a model on [OpenRouter](https://openrouter.ai). An administrator turns it on under
**Admin → Settings → AI rulings** with an OpenRouter API key and any model that supports tool
calling; saving checks both with OpenRouter, and the key is stored encrypted. Each question's cost
is recorded per member, and the same page shows this month's spending. **Monthly limit per member
(USD)** caps each member's spending per calendar month (UTC); administrators have no cap. Each
member can also ask at most 30 questions every 10 minutes.

The server downloads the Comprehensive Rules from Wizards of the Coast shortly after it starts and
checks for a new release daily. **Admin → Catalog → Comprehensive Rules** shows the installed
release, and **Check for new rules** checks now. `RULES_SYNC_ENABLED=false` turns the downloads
off.

### Scheduling games from Discord

With the Discord bot set up, `/newgame [start] [min_players] [title] [format]` posts a queue with
**Join**, **Maybe**, and **Leave** buttons in a thread. The minimum is three players unless set
(two to ten). Without `start`, the bot posts a webcam-table link as soon as the minimum joins. A
scheduled queue starts or expires at its start time; if it is short, the bot pings the Maybe list
and waits 15 minutes. The host or a Discord Administrator can change the time, cancel, or run
`/close` in the thread. See
[Discord integration](discord-integration.md#webcam-table-queues-with-newgame) for time syntax,
permissions, and alerts.

## Tabletop

**Games → Tabletop** turns one phone or tablet into the life counter for a table that plays in
person. Pick a format, two to six players, how they sit around the device, and starting life. Each
player then taps their seat to sit down as a playgroup member (with one of their decks) or as a
guest. **Same side** puts two players on one edge, so a sideways phone reads like an ordinary
landscape life counter.

Tap the left half of a seat to lose life and the right half to gain it; hold to change it by ten.
Each seat's counters button tracks poison and conceding. While it is open, every opponent's seat
becomes a commander damage counter: tap an opposing commander to add the damage it dealt, which
also comes off life. The hub in the middle rolls for the first player, tracks turns and the game
clock, and ends the game.

A finished game goes into game history only when every seat is a member and the format is one
history records. Games with a guest or an empty seat, and Standard, Pauper, or Canadian Highlander
games, just end. In a recorded game, a player who goes out is asked on their own seat how the game
was ([Salt](#salt)), and the **Game over** dialog lists every player's faces again. The game in progress is saved in the browser, so a reload or reopening the
installed app picks it up again. The screen stays awake while the board is open.

## Game history

### Salt

Every player can rate how much they enjoyed a game, from awful to rad, with one click. At the
webcam table a small prompt appears when your seat is knocked out, and again once the game is
recorded if you have not rated it yet (on the game's page, or at the table after a rematch); the
cross dismisses it. Ratings given at the table are saved with the game. On a game's page each
player's face shows on their seat, your own seat has the five faces to rate or change your rating,
and the header shows the table's average mood, which the games list shows per game too. The games
list's **Table mood** filter (under More filters) shows only games whose average rounds to one face.

**Salt** in the menu shows the playgroup's average mood, the saltiest game (the lowest average
among games at least two players rated), how ratings spread, mood by month, the saltiest and least
salty commanders (ranked by how the rest of the table rated games against them,
once they have three such ratings), and players ranked by how they rated their own games. Player
profiles and deck pages show their own average and their opponents' average.

### Kills

When logging or editing a game, use each player's **Kills** picker to select who they killed. A
victim can have only one killer. **Total kills** counts those selections; enter a larger total, or
only a total, when some victims are unknown. The total can't be lower than the number of named
victims. When logging a game, a blank total records 0; when editing, a blank total means unknown.
Game details show named victims and any unknown remainder. The table's **Record game result**
dialog has the same kill and MVP card fields.

On a player's profile, **Kill rivalries** show who they killed most and who killed them most in the
selected date range, counting named victims only. Rivalry cards and head-to-head counts open the
matching games with the same date range. Commander rivalry counts open the games against that
commander; a mirror match counts each matching seat but lists the game once.

## Playgroups

Every player, deck, and game belongs to one playgroup. Playgroups are private: members see only the
playgroups they belong to, and a member can belong to several. The switcher next to the logo
picks the playgroup you are viewing, and the choice is saved to your account. Games you log,
import, or play at a table go into that playgroup. A member of no playgroup sees an empty page
until an administrator adds them or they accept an invite link.

Administrators manage playgroups under **Admin → Playgroups**: create (the creator becomes the first
member), rename, add and remove members, and delete. Administrators see every playgroup there but
can view only the ones they belong to. A playgroup can be deleted only when it has no games and no
decks, and the last playgroup can't be deleted. Deleting removes its players and memberships.

**Move to another playgroup** on a game's page (administrators only) moves a game logged in the
wrong playgroup into another one the administrator belongs to. Each seat is matched to a player
there by account, then Discord identity, then name, and to that player's deck with the same name;
missing players and decks are created. The original players and decks stay where they were. The
move is refused when a seat's name matches a player there who is linked to a different account or
Discord identity.

A playgroup can claim a **Discord server ID** so the bot records that server's games into it.
Games from a server no playgroup claims go to the oldest playgroup. To serve several Discord
servers from one bot, leave `DISCORD_GUILD_ID` unset and set each server's ID on its playgroup.
`DISCORD_NEWGAME_ROLE_ID` holds one role, so `/newgame` alert pings work in only one server.

### Invite links

Each playgroup on **Admin → Playgroups** has an invite link for Discord sign-in. **Create link**
shows it once: copy it before leaving the page, because the server keeps only a hash. Anyone with
the link joins that playgroup:

- someone new gets an account, even with **Open registration** off;
- a signed-in member is added to the playgroup and switched to it at once.

Links do not expire and have no use limit, so treat a link like a password. Accounts that sign in
with Discord get a player in the playgroup automatically; an administrator links a password account
to a player. **Rotate link** replaces the link and revokes the old one immediately, including
Discord sign-ups still in progress. Members who already joined keep their access, and disabled
accounts stay blocked. The secret travels in the URL fragment, so it does not appear in server or
proxy request logs.

## Decks

### Deck-list links

A deck can link to a public Moxfield or Archidekt deck, or to a deck on a self-hosted ManaVault
when the server sets `MANAVAULT_URL`. Pasting a link previews the deck, and **Use these details**
fills in its name, commanders, and color identity. ManaVault links from any other address are not
fetched and stay plain links. The server reads the list live from the host each time (cached for
five minutes) and keeps no copy. Commander and mainboard cards count; sideboards and maybeboards
are left out. Private or deleted lists, and decks without a supported link, have no list. Moxfield
has no supported public API and sometimes blocks server requests.

### Deck hosts

Under **Settings → Profile → Deck hosts**, members can save a Moxfield username, an Archidekt
username, and a ManaVault instance URL with a personal API key (created under ManaVault's
**Settings → Personal API keys**). Your decks on those hosts then appear under **Quick pick from my
hosted decks** in deck forms. Listings are cached for five minutes, and a host that fails to list
reports why without hiding the others. Each host's listing stops after 20 pages, 500 decks, 2 MB,
or 15 seconds and says so.

The ManaVault API key is stored encrypted and is never shown again. Leave the field blank to keep
it, or use **Remove key** to delete it. Because a keyed listing can include unshared decks, a quick
pick from it fills the form directly instead of fetching a public share link.

The server only contacts a member's ManaVault instance at an `https://` origin with no path.
Private, loopback, link-local, and Tailscale (100.64.0.0/10) addresses are blocked unless the
exact host name is in `MANAVAULT_ALLOWED_HOSTS`, which also allows `http://` for that host.
`MANAVAULT_ALLOW_INSECURE_URLS=true` allows `http://` for public hosts only. Redirects are not
followed.

**Sync hosted decks**, on your player profile and the deck chooser once a deck host is saved, adds
your hosted decks marked as Commander to your player's decks; other or unknown formats are
skipped. A hosted deck matches a local deck with the same deck-list link, then the same name, then
an unlinked deck with the same commanders; anything else becomes a new deck. Matched decks keep
their local name, commanders, art, colors, archive status, and chooser setting, and only gain a
deck-list link if they had none. Sync never removes decks.

### Commander printings and Game Changers

On a deck's page, **Choose printing** under the commander or partner picks the printing that
matches your card's artwork; **Save deck** keeps it and **Use catalog default** removes it. The
choice changes only the art, survives catalog refreshes, and is cleared when that commander
changes.

A **Game Changer** badge marks a commander on the Commander Brackets Game Changers list, from
Scryfall's flag in the local catalog. No badge means the catalog has no positive flag. Badges are
per commander; the app keeps no deck lists, so it can't count a whole deck's Game Changers.

## Deck chooser

**Decks → Choose a deck** picks one of your player's active decks. Decks you haven't played lately,
have skipped, or have played less are more likely: each deck's weight is hours since it was last
played, times its skips plus one, divided by its plays plus one. Never-played decks count as 30
days staler than the stalest played deck. **Skip** counts a skip and picks again; choosing a deck
clears its skips. Plays and last-played dates come from game history.

**Customize** replaces those weights with your own Lua script. A script defines
`weight(deck, ctx)` and sees each deck's plays, skips, results, recent games, and colors, plus the
local time, games played today and tonight, and your recent history. The editor previews every
deck's chance before you save. Scripts run sandboxed on the server with memory, instruction, and
time limits; a script that fails falls back to the built-in weights for that pick. See
[Deck chooser scripts](deck-chooser-scripts.md) for every field and the examples.

## Appearance

**Settings → Appearance** picks a color palette (Claret, Nord, Catppuccin, Tokyo Night, Gruvbox,
Everforest, Kanagawa, Night Owl, Dracula, Rosé Pine, Solarized, or Monochrome) and a surface style
(**Liquid glass** or **Classic**). Both are saved to your account, so every device shows them from
the first paint. Light or dark mode is set per device with the toggle in the navigation.
