# Mythic Track import

Administrators can move a playgroup's history from [Mythic Track](https://www.mythictrack.com/) at **Import → Mythic Track** in the main navigation. Mythic Track has no export feature, so the import page shows a short browser-console snippet that saves the game list its own client fetches.

## Exporting from Mythic Track

1. Sign in at mythictrack.com in a desktop browser.
2. Open the developer console (F12, or ⌥⌘J on macOS) and paste the snippet from the import page. It calls `POST https://www.api.mythictrack.com/api/games/get` with your session cookie and downloads the response as `mythic-track-games.json`.
3. Upload that file (or paste its contents) on the import page, preview, then confirm.

The endpoint only returns games the signed-in user played in. Have each member of the playgroup export and import their own file if you want complete coverage; games already present are skipped, so overlapping exports are safe.

The API routes are `POST /api/imports/mythic_track/preview` and `POST /api/imports/mythic_track`, both accepting `{"json": "..."}` and restricted to administrators. The preview response has the same shape as the CSV preview plus a `warnings` list for games that are skipped rather than rejected.

## Mapping

| Mythic Track | The Gathering |
| --- | --- |
| Game `id` (GUID) | `games.external_id` with `source = "mythic_track"`, so re-imports skip existing games |
| `createdOn` | `played_at`, read as UTC (Mythic Track stores no offset) |
| `gameTimeInMinutes`, `totalTurns` | `duration_minutes`, `turns` |
| `name` + `notes` | `notes`, joined when both are present |
| Player `discordUserId`, then `name` | Existing player with that Discord ID; otherwise a case-insensitive name match; otherwise a new player. A name-matched player without a Discord ID gains the exported one. |
| Commander `deckName`, else commander name(s) | Deck name, owned by that player |
| Commander / partner `scryfallId`, `name`, `colors`, `decklistUrl` | Deck commander, partner, colour identity, and decklist link |
| `turnOrder` | Seat, renumbered 1..n (missing values sort last) |
| `isWinner` | One winner → win/loss; no winner → all draw; more than one winner → error |
| `keyCards` | The first key card becomes the winner's MVP card (name and Scryfall ID). Any further key cards, or all of them in a draw, are appended to notes as `Key cards: …` |

Only games with `gameStatus` 3 (complete) are imported; in-progress and unstarted games appear as warnings.
