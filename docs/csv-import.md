# CSV game import

Administrators can import past games at **Import** in the main navigation. The flow has two phases:

1. Upload a `.csv` file or paste its contents, then preview it. The preview identifies existing and new players/decks and reports errors by CSV line.
2. Confirm a valid preview. The entire file is committed in one database transaction; if any game fails, none of its players, decks, or games are saved.

The API routes are `POST /api/imports/csv/preview`, `POST /api/imports/csv`, and `GET /api/imports/csv/sample`. The POST routes accept JSON shaped as `{"csv":"..."}`. They are restricted to administrators.

## Native format

[Download a sample CSV](/api/imports/csv/sample), or use these columns:

| Column | Required | Meaning |
| --- | --- | --- |
| `game_id` | yes | A value that groups all seats from one game. It only needs to be unique within the file. |
| `date` | yes | ISO 8601 date/time or `YYYY-MM-DD`. A date without a time becomes noon UTC. |
| `player` | yes | Player name. Matching is case-insensitive; a missing player is created. |
| `deck` | yes | Deck name, unique for that player. A missing deck is created. |
| `commander` | yes | Commander name used when creating the deck. |
| `seat` | yes | Consecutive number from 1 through the number of players. |
| `result` | yes | `win`, `loss`, or `draw`. A game has one winner and all other players lose, or every player draws. |
| `mvp_card` | no | MVP/key card for this seat. |
| `duration_minutes` | no | Positive whole number. Use the same value on every row for a game. |
| `turns` | no | Positive whole number. Use the same value on every row for a game. |
| `notes` | no | Game notes. Use the same value on every row for a game. |

Each game needs 2–6 rows. A player can only appear once in a game.

```csv
game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes
friday-001,2026-09-18,Alice,Birds of a Feather,"Kangee, Sky Warden",1,win,Swan Song,75,10,Friday Commander
friday-001,2026-09-18,Bob,Goblins,Krenko Mob Boss,2,loss,,75,10,Friday Commander
```

## Mythic Track spreadsheet

The importer also recognizes the headers from Mythic Track's official [spreadsheet import template](https://docs.google.com/spreadsheets/d/1f-ekY6JZ5N92MBYYl0yMuyOvtyvm-YxfWlw6MTB7RqQ/edit?usp=sharing). Export the `Games` sheet starting with its header row (`Date`, `Player1`, `Player1Commander`, and so on), then upload that CSV directly.

Mythic Track's published template has commander columns but no deck-name columns, so each imported deck uses its commander name as the deck name. Its blank `Winner` value is imported as an all-player draw. The template supports up to four players; the native format supports up to six.

## Re-importing

Imported games have source `csv`. The importer hashes each normalized game group into a deterministic external ID. Re-importing the same file skips games already present and reports the created/skipped counts. Changing a game's data changes its identity and creates a new game rather than editing the earlier import.
