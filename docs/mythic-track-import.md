# Mythic Track import

Administrators bring a playgroup's history over from [Mythic Track](https://www.mythictrack.com/)
at **Import → Mythic Track**. Mythic Track has no export button, so the import page gives a
browser-console snippet that downloads the game list Mythic Track's own site loads.

## Exporting from Mythic Track

1. Sign in at mythictrack.com in a desktop browser.
2. Open the developer console (F12, or ⌥⌘J on macOS) and paste the snippet from the import
   page. It calls `POST https://www.api.mythictrack.com/api/games/get` with your session and
   downloads the result as `mythic-track-games.json`.
3. Upload that file (or paste its contents) on the import page, preview, and confirm.

Mythic Track returns only games the signed-in user played in. For full coverage, have each
member export their own file and import them all. Games already imported are skipped, so
overlapping files are safe.

The API routes are `POST /api/imports/mythic_track/preview` and `POST /api/imports/mythic_track`.
Both take `{"json": "..."}` and need an administrator; committing also needs a password
confirmation within the last ten minutes. The preview has the same shape as the CSV preview,
plus a `warnings` list of skipped games.

## Mapping

| Mythic Track | The Gathering |
| --- | --- |
| Game `id` (GUID) | `games.external_id` with `source = "mythic_track"`, so re-imports skip existing games |
| `createdOn` | `played_at`. A date-only midnight value becomes noon UTC; real times and explicit offsets are kept. |
| `gameTimeInMinutes`, `totalTurns` | `duration_minutes`, `turns` |
| `winCondition` | Win condition: `1` Damage through `11` Concede, `8` Draw, and `99` or any other value Unknown, following Mythic Track's own numbering |
| `name` + `notes` | `notes`, joined when both are present |
| Player `discordUserId` | The player with that Discord ID, or a new player with it. A taken name gets a suffix such as `Alice (2)`. |
| Player `name` (no Discord ID) | A case-insensitive name match, or a new player |
| Commander `deckName`, else commander name(s) | Deck name, owned by that player |
| Commander and partner `scryfallId`, `name`, `colors`, `decklistUrl` | The deck's commander, partner, color identity, and deck link |
| `turnOrder` | Seat, renumbered 1 through n (missing values sort last) |
| `isWinner` | One winner: win and losses. No winner: every seat draws. More than one: the game is skipped. |
| `keyCards` | The first key card becomes the winner's MVP card. Further key cards, or all of them in a draw, are added to the notes as `Key cards: …` |

Only games with `gameStatus` 3 (complete) are imported; unstarted and in-progress games are
listed as warnings.

Mythic Track stores a partner pair as one commander named `A || B (Partners)`. The importer
splits it into commander and partner, and names the deck `A / B` when Mythic Track had no
deck name.

## Skipped games

The importer skips games with fewer than two or more than six players, a blank player name,
the same player in two seats, or more than one winner. Each skipped game is a warning with
its name, date, and players, for example
`skipped: Daniel is listed twice (03/17/2025 - Commander (EDH) - Game 1, 2025-03-17, players: Daniel, Reality, Daniel, Matt)`,
and the rest of the file still imports. Fix the game in Mythic Track and export again, or log
it by hand. Only an entry that is not an object, a missing `id`, or an unreadable `createdOn`
rejects the file.

## Linking cards to the catalog

Imported decks and MVP cards may arrive with names but no Scryfall IDs, and then have no art
or color identity until they are linked to the local card catalog. Linking runs after every
import and after each catalog sync. It matches names exactly (ignoring accents), falls back to
one face of a multi-faced card, fills missing commander, partner, and MVP card IDs, and
fills an empty color identity from the linked cards. Run it again from
**Admin → Catalog → Link imported cards to the catalog** (`POST /api/admin/catalog/backfill`)
or with:

```sh
the-gathering catalog-backfill   # in the container: /app/bin/the-gathering catalog-backfill
```

The result lists names it could not match, so you can fix those decks by hand.

## Players and decks

Imported players do not need accounts. The preview lists the players it will create, and every
commander a player used becomes one of their decks, so the deck is offered the next time a game
is logged.

A member who signs in with Discord takes over the imported player that has their Discord ID.
Otherwise they get a new player. An administrator can then link the account to the imported
player from **Admin → Users** (the **Player** select on each user), which moves games logged
under the account's player onto the imported one. To fold together players Mythic Track kept
apart (say `Drew` and `waxpoetik`), use **Merge into another player** on a player's page: games,
decks, and the account link move to the chosen player. A merge is refused when both players sat
in the same game.
