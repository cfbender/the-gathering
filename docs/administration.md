# Administration

Accounts, sign-in, audit history, live logs, and the card catalog. Installing and configuring the
server is covered in [Installation](installation.md).

Every **Admin** page, and other sensitive actions such as import commits and player merges, needs
a sign-in within the last ten minutes. The app asks the administrator for their password, or a
Discord member to approve with Discord again. Import previews and sample downloads don't need it.

## Accounts and registration

The first account registered in the browser becomes the administrator. It signs in with a
password; everyone else signs in with Discord. New Discord accounts are accepted only while
**Open registration** is on under **Admin → Settings**, or through a playgroup
[invite link](features.md#invite-links). Members who already have an account can always sign in.
To create the administrator without the browser, or to add another password administrator, use
`THE_GATHERING_ADMIN_USERNAME`/`THE_GATHERING_ADMIN_PASSWORD` or `the-gathering create-admin`
([Installation](installation.md#maintenance-commands)). Passwords are 12 to 72 characters.

**Admin → Users** lists accounts. For each one an administrator can:

- **Disable** it, which signs it out on every device. **Enable** lets it sign in again; old
  sessions stay ended.
- **Sign out everywhere** without disabling it.
- **Delete user**, only when its player has no games. This removes the account, its sessions, its
  player, and its unused decks. Games it recorded for other players stay. Disable accounts with
  game history instead.
- Link it to a player, for example one created by an import.

**Admin → Player identities** lists every player (archived ones too) with their Discord ID and
linked account. Search by player name, Discord ID, or username. **Unlink identity** detaches the
Discord identity and the account from a player before you merge players; the player's games and
decks stay. The account and its Discord sign-in are unchanged, so its next sign-in or import may
create a new player; link it to the right one under **Admin → Users**.

**Detailed statistics from** on **Admin → Settings** sets a cutoff for statistics your group may
not have recorded from the start. Earlier games still count toward wins and losses, but their seat
positions, game length, turn counts, and MVP cards are left out. See [Statistics](stats.md).

Members can create personal API keys under **Settings → API keys** to read games from scripts. A
key acts as its owner, stops working when the account is disabled, and is limited to 120 requests
a minute. See [Personal API keys](api.md).

For Discord sign-in and the bot, see [Discord integration](discord-integration.md). To bring in
existing history, see [CSV import](csv-import.md) and [Mythic Track import](mythic-track-import.md).

## Sessions and rate limits

A session lasts 14 days and renews after 7, so members who visit at least every two weeks stay
signed in. Discord skips its consent screen once a member has authorized the app. Signing out ends
the session on that device only. Changing the administrator password signs that account out on
every other device.

Password sign-in and first-time registration allow 10 attempts per client address every 5
minutes, sign-in also 10 attempts per username from any address, and 100 attempts across the
server. Password re-authentication for admin pages allows 5 attempts per account and address every
5 minutes. Further attempts get `429 Too Many Requests` with a `retry-after` header. Discord
sign-in is not rate limited by the server.

Behind a reverse proxy every request seems to come from the proxy, so all clients share one
limit. Set `TRUST_PROXY_HEADERS=true` and `TRUSTED_PROXIES` to your proxy's address
([Installation](installation.md#7-https-and-the-reverse-proxy)). The server believes forwarded
addresses only from those proxies, so clients that reach port 4000 directly can't choose their own.

## Audit history

**Admin → Audit log** lists changes made by members and administrators: who did it (as they were
named at the time), the operation and target, the request ID, the outcome, and before and after
snapshots of each changed record. Search by username, operation, or target, and filter by outcome.
Deleting or renaming an account keeps its name in past entries.

What it records:

- Changes to users, playgroups and their members, players, decks, games and their seats, server
  settings, AI rulings settings, API keys, sheet import receipts, pending Discord games, and
  Discord result drafts, from the web app, API, Discord sign-in, and Discord commands and buttons.
- Snapshots commit or roll back with the change itself, and cascaded deletions record each removed
  row. A failed request that still saved something is listed with its changes.
- Discord operations show as **Accepted**; their changes show what happened. **Unknown / in
  progress** means the request is still running or its final status was not saved.

What it doesn't record:

- Requests that changed nothing: sign-ins that don't create or change an account, webcam table
  controls, deck chooser picks, saves with no changes, and rejected requests.
- Live table state, scheduled `/newgame` queues, card images and recognition corrections, catalog
  and cache data, and effects outside the database, such as Discord messages.
- Passwords, credential hashes, session tokens, invite secrets, API key and ManaVault key values,
  request bodies, and query strings. Snapshots show only whether a credential is set, and a change
  to a secret alone, such as rotating an invite link, is not listed.
- Changes from background jobs or direct SQL. They are saved without an operation and don't
  appear in the list.

History is kept forever in the application database; the app has no way to delete it. It has no
undo, and it is not tamper-proof: anyone with access to the database file can change it. Keep
backups. Portable game exports don't include it.

## Server logs

**Admin → Server logs** streams the server's log while the page is open, filtered by the same
`LOG_LEVEL` as its standard output. Nothing is replayed or stored: the page keeps the newest 200
messages, says when messages were dropped because it fell behind, and **Clear** empties only that
page. The server rechecks access every five seconds, so signing out, losing administrator rights,
or an expired re-authentication ends the stream. Your reverse proxy must not buffer it
([Installation](installation.md#7-https-and-the-reverse-proxy)).

Fields whose names look like secrets (`password`, `token`, `cookie`, and similar) and control
characters are stripped, but message text is not redacted.

## Card catalog

The server keeps its own copy of Scryfall's card data: every printing, plus rulings. Card search,
printing pickers, prices (as of the last sync), and rulings read only this local copy. It downloads
the catalog when it first starts and then weekly (`CATALOG_SYNC_INTERVAL_HOURS`). A new catalog
replaces the old one only once it has fully downloaded. Printings Scryfall no longer lists are
removed unless a deck uses them. **Admin → Catalog** shows the last sync; **Sync now** runs one, as
does:

```sh
docker compose exec the-gathering /app/bin/the-gathering catalog-sync
```

After each sync and each CSV or Mythic Track import, the server links decks and MVP cards that
only have a card name to catalog cards, filling in missing card IDs and color identities. **Link
imported cards to the catalog** on **Admin → Catalog** or `the-gathering catalog-backfill` runs it
alone.

Each card shows its preferred printing: English, on paper, not digital-only, not a promo, then the
newest. A card can be a commander when its front face is a legendary creature, Vehicle, or
Spacecraft, or its text says it can be your commander. Backgrounds can't be commanders on their own.

### Card images

Card images come from Scryfall through the server, and only signed-in members can fetch them. The
server caches them in `DATA_DIR/card-images` (`/data/card-images` in Docker):

- The cache holds at most 512 MiB, removing the oldest files first, and each file expires after
  30 days. It is safe to leave out of backups or to delete while the server is stopped.
- Uncached images need internet access to Scryfall. The server downloads at most four at a time,
  and pauses at least 30 seconds when Scryfall says to slow down.
- Only JPEG images up to 2 MiB from `cards.scryfall.io` are accepted, and redirects are not
  followed.
