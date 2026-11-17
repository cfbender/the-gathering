# CSV game import

Administrators import past games at **Import** in the main navigation:

1. Upload a `.csv` file or paste its contents, then preview it. The preview shows which
   players and decks exist and which will be created, and reports errors by CSV line.
2. Confirm a valid preview. The whole file is saved in one transaction: if any game fails,
   nothing is saved.

The API routes are `POST /api/imports/csv/preview`, `POST /api/imports/csv`, and
`GET /api/imports/csv/sample`. The POST routes take `{"csv": "..."}`. All three need an
administrator, and committing also needs a password confirmation within the last ten minutes.

## Native format

[Download a sample CSV](/api/imports/csv/sample), or use these columns:

| Column | Required | Meaning |
| --- | --- | --- |
| `game_id` | yes | Groups the rows of one game. It only needs to be unique within the file. |
| `date` | yes | ISO 8601 date and time, or `YYYY-MM-DD`. A date without a time becomes noon UTC. |
| `player` | yes | Player name, matched case-insensitively. A missing player is created. |
| `deck` | yes | Deck name. The player's deck with that name is reused, else their deck with the same commander (and partner), else a new deck. |
| `commander` | yes | Commander name, used when creating the deck. |
| `seat` | yes | Turn order, numbered 1 through the number of players. |
| `result` | yes | `win`, `loss`, or `draw`. A game has one winner and the rest lose, or every player draws. |
| `mvp_card` | no | MVP card for this seat. |
| `duration_minutes` | no | Positive whole number, the same on every row of a game. |
| `turns` | no | Positive whole number, the same on every row of a game. |
| `notes` | no | Game notes, the same on every row of a game. |
| `kills` | no | The player's kill total, 0–5. `0` is a known zero; blank is unknown on create and keeps the stored value on update. |
| `partner` | no | Partner commander name. |
| `win_condition` | no | One of the keys below, the same on every row of a game. |
| `action` | no | `create` (default), `update`, or `skip`. |
| `source`, `external_id` | for updates | The existing game's original identity, such as `mythic_track` and its GUID. Give both. |
| `portable_id` | for updates | The game's UUID from a portable export. Never use a server's numeric game ID. |

Each game has 2–6 rows, and a player appears once per game.

Win-condition keys: `damage`, `infinite_combo`, `mill`, `poison`, `alternate_win_con`,
`hard_lock`, `commander_damage`, `draw`, `non_combat_damage`, `combat_damage`, `concede`,
`unknown`. Leave it blank when not recorded, and put details such as the combo in the notes.

```csv
game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes
friday-001,2026-09-18,Alice,Birds of a Feather,"Kangee, Sky Warden",1,win,Swan Song,75,10,Friday Commander
friday-001,2026-09-18,Bob,Goblins,Krenko Mob Boss,2,loss,,75,10,Friday Commander
```

## Mythic Track spreadsheet

The importer also reads the `Games` sheet of Mythic Track's
[spreadsheet import template](https://docs.google.com/spreadsheets/d/1f-ekY6JZ5N92MBYYl0yMuyOvtyvm-YxfWlw6MTB7RqQ/edit?usp=sharing).
Export it as CSV starting at its header row (`Date`, `Player1`, `Player1Commander`, and so on)
and upload it as is.

The template has no deck names, so each deck is named after its commander. A blank `Winner`
imports as a draw for every player. The template holds up to four players. For exports from
Mythic Track itself, see [Mythic Track import](mythic-track-import.md).

## Re-importing

Imported games have source `csv` and an external ID hashed from the game's rows. Importing the
same file again skips games already present and reports created and skipped counts. A game whose
rows changed gets a new identity and is created as a new game. To change an imported game, use
an update.

## Correcting existing games

Set `action=update` and identify the game with `source` + `external_id` or with `portable_id`.
If you give both, they must point to the same game. A missing target, conflicting identities,
or two groups targeting one game block the whole file; an update never creates a game.

Give the complete corrected seat list and date. The preview shows before and after values and
separates creates, updates, and unchanged games. Corrections keep the game's ID, source
identity, and portable ID. Players who stay keep their seat notes, MVPs, and eliminations; a
removed player's seat is deleted, along with eliminations credited to them. Seat order can
change. A deck name that matches the player's deck with different commanders blocks the
import; give the new deck a different name.

Blank optional fields keep the stored values. A given `notes` value replaces the game's notes,
so include any old notes you want to keep. There is no way to clear a field from CSV. A
date-only value becomes noon UTC; supply `YYYY-MM-DD` to repair a date-only game stored at
midnight UTC, which shows as the previous day in American time zones.

The preview returns a `revision`. When any group is an update, confirm with `csv` and that
`revision`; if the file or the database changed since the preview, preview again. Repeating an
unchanged correction skips the game. Export a backup before correcting production history.

## Transfer between servers

Any member can download the playgroup's history from the **…** menu on the **Games** page with
**Export history (JSON)**. On the other server, an administrator opens **Import → Transfer**,
chooses the file, and selects **Preview transfer**, then confirms after reviewing the new and
existing player, deck, and game counts.

The export holds every player, deck, and game, including unused and archived ones: deck
chooser settings, notes, kills (keeping zero apart from unknown), results, seat order,
eliminations, duration, turns, MVPs, the referenced card metadata and chosen printings, and
Google Sheet reconciliation receipts. Artwork is kept as URLs, not files.

It is not a backup. It leaves out accounts, passwords, sessions, Discord IDs, account links,
server settings, and audit history, so imported players start unlinked. Keep exports private:
they include player names and notes.

Transfers only add. The preview checks the whole file in a rolled-back transaction, and
confirming checks it again and saves everything or nothing. Existing records are never
overwritten or deleted:

- Players match by case-insensitive name, and decks by owner and case-insensitive name;
  existing settings win. A deck with the same name but different commanders blocks the
  transfer until one is renamed. Check names before combining unrelated playgroups.
- Games keep a portable UUID, so a repeated transfer does not duplicate them, even after edits.
  Games imported on both servers from the same CSV, Mythic Track, or Discord source are also
  recognized. Games logged by hand on both servers are not.
- Card metadata is restored without contacting Scryfall, and existing metadata is kept. Sheet
  receipts stop unchanged sheet rows from being applied again.

`GET /api/exports/portable` downloads the file for any signed-in member.
`POST /api/imports/portable/preview` and `POST /api/imports/portable` need an administrator
and take `{"json": "<file contents>"}`; confirming also needs a recent password confirmation.
Files declare `"format": "the-gathering"` and `"version": 1`; other versions are rejected.

## Google Sheet reconciliation API

The app no longer has a screen for reconciling a one-game-per-row Google Sheet; transform such
a sheet into the seat-per-row CSV above instead. The API remains:
`POST /api/imports/sheet/preview` with `{text, players, decks, actions}`, then
`POST /api/imports/sheet` with the same body plus the preview's `revision` (administrator and
recent password confirmation).

- `text` is the pasted sheet with `Date`, `Winner`, `Deck`, one kill column per player,
  `Win Con`, `Other Decks`, and `Notes`. Dates may be `M/D/YY` (2000–2099), `M/D/YYYY`, or ISO.
- `players` maps sheet names (participants and kill-column headers) to player IDs or `"new"`.
- `decks` maps JSON-encoded `[sheetPlayer, sheetDeck]` keys to deck IDs or `"new"`.
- `actions` maps preview row keys to a game ID, `"create"`, or `"skip"`.

The preview matches each row to a game with the same mapped players on the same UTC day, then
the day before or after, using deck names or commanders to break ties. Ambiguous and invalid
rows are skipped for review. Updates keep the game's IDs, source identity, turn order,
duration, turns, MVPs, and seat notes; sheet results and non-empty notes replace the stored
values. In the sheet a blank kill cell means zero, and a blank or `N/A` winner means every
listed player drew. New games are placed at noon UTC in sheet order. Receipts remember each
applied row, so the same row is never applied twice, and a database change after the preview
invalidates the `revision`.
