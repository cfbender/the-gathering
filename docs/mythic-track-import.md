# Mythic Track import

Administrators can move a playgroup's history from [Mythic Track](https://www.mythictrack.com/) at **Import → Mythic Track** in the main navigation. Mythic Track has no export feature, so the import page shows a short browser-console snippet that saves the game list its own client fetches.

## Exporting from Mythic Track

1. Sign in at mythictrack.com in a desktop browser.
2. Open the developer console (F12, or ⌥⌘J on macOS) and paste the snippet from the import page. It calls `POST https://www.api.mythictrack.com/api/games/get` with your session cookie and downloads the response as `mythic-track-games.json`.
3. Upload that file (or paste its contents) on the import page, preview, then confirm.

The endpoint only returns games the signed-in user played in. Have each member of the playgroup export and import their own file if you want complete coverage; games already present are skipped, so overlapping exports are safe.

The API routes are `POST /api/imports/mythic_track/preview` and `POST /api/imports/mythic_track`, both accepting `{"json": "..."}` and restricted to administrators. Committing an import also requires reauthentication within the previous ten minutes. The preview response has the same shape as the CSV preview plus a `warnings` list for games that are skipped rather than rejected.

## Mapping

| Mythic Track | The Gathering |
| --- | --- |
| Game `id` (GUID) | `games.external_id` with `source = "mythic_track"`, so re-imports skip existing games |
| `createdOn` | `played_at`; naive date-only midnight values are normalized to noon UTC, while actual times and explicit offsets are preserved |
| `gameTimeInMinutes`, `totalTurns` | `duration_minutes`, `turns` |
| `winCondition` | Canonical win condition (`1` Damage through `11` Concede, with `8` Draw and `99`/unrecognized values Unknown), using the enum extracted from Mythic Track's public `MtgDataTracker.Shared` assembly |
| `name` + `notes` | `notes`, joined when both are present |
| Player `discordUserId`, then `name` | Existing player with that Discord ID; otherwise a case-insensitive name match; otherwise a new player. A name-matched player without a Discord ID gains the exported one. |
| Commander `deckName`, else commander name(s) | Deck name, owned by that player |
| Commander / partner `scryfallId`, `name`, `colors`, `decklistUrl` | Deck commander, partner, colour identity, and decklist link |
| `turnOrder` | Seat, renumbered 1..n (missing values sort last) |
| `isWinner` | One winner → win/loss; no winner → all draw; more than one winner → the game is skipped |
| `keyCards` | The first key card becomes the winner's MVP card (name and Scryfall ID). Any further key cards, or all of them in a draw, are appended to notes as `Key cards: …` |

Only games with `gameStatus` 3 (complete) are imported; in-progress and unstarted games appear as warnings.

Mythic Track writes a partner pair as one commander named `A || B (Partners)`. The importer splits that into the deck's commander and partner, and names the deck `A / B` when Mythic Track had no separate deck name.

## Linking cards to the catalog

Imported decks and MVP cards arrive with card names but not always Scryfall IDs, so they have no art or colour identity until they are linked to the local card catalog. The link runs automatically after every import and after each catalog sync, matching names exactly (accents ignored) and falling back to the front face of double-faced cards. It fills missing commander, partner, and MVP card IDs, and fills an empty colour identity from the linked cards; existing colour identities are left alone. Run it again by hand from **Admin → Users → Link imported cards to the catalog** (also `POST /api/admin/catalog/backfill`) or with:

```sh
mise exec -- mix the_gathering.catalog.backfill
```

Names it cannot match are listed in the result so you can fix the deck by hand.

## Skipped games

Mythic Track lets you save games The Gathering cannot represent: fewer than two or more than six players, a blank player name, the same player in two seats, or several winners. Those games are listed as warnings with the game's name, date, and players (for example `Game 213: skipped: Daniel is listed twice (03/17/2025 - Commander (EDH) - Game 1, 2025-03-17, players: Daniel, Reality, Daniel, Matt)`) and the rest of the file still imports. Fix the game in Mythic Track and re-export, or log it by hand afterwards. Only a missing `id` or an unreadable `createdOn` rejects the whole file.

## Players and decks

Nobody needs an account to appear in imported games. The preview lists the players it will create; each becomes a plain player record, and every commander a player used becomes a deck owned by that player so it can be suggested the next time a game is logged.

When someone later signs in with Discord, they get their own player record. An administrator can point their account at the imported player instead from **Admin → Users** (the *Player* select on each user), which moves any games already logged under the account's player onto the imported one. Duplicates that Mythic Track kept apart (say `Drew` and `waxpoetik`) can be folded together from the *Merge into another player* card on a player's page; games, decks, and the account link all move to the chosen player. Merging refuses when both players sat in the same game.
