# CSV game import

Administrators can import past games at **Import** in the main navigation. The flow has two phases:

1. Upload a `.csv` file or paste its contents, then preview it. The preview identifies existing and new players/decks and reports errors by CSV line.
2. Confirm a valid preview. The entire file is committed in one database transaction; if any game fails, none of its players, decks, or games are saved.

The API routes are `POST /api/imports/csv/preview`, `POST /api/imports/csv`, and `GET /api/imports/csv/sample`. The POST routes accept JSON shaped as `{"csv":"..."}`. They are restricted to administrators; committing an import also requires reauthentication within the previous ten minutes.

## Transfer between The Gathering instances

Use **Import → Export / transfer → Download JSON export** to download a versioned JSON file. On another instance running a version that supports portable transfers, open the same page, choose the file, and select **Preview transfer**. Review the new/existing player, deck and game counts, then confirm. No CSV conversion or manual ID mapping is needed.

The export includes all players, decks and games, including unused/archived records, deck selection settings, notes, kills (preserving zero versus unknown), results, seat order, eliminations, duration, turns and MVPs. It also includes referenced card metadata, selected printings and Google Sheet reconciliation receipts. Artwork URLs are preserved; image files are not embedded.

This is a gameplay transfer, **not a full database/account backup**. It excludes accounts, passwords, sessions, Discord IDs, account links, server settings and audit timestamps. New players are unlinked and can be linked to accounts afterward. Keep exports private: player names and notes are included.

Imports are additive and atomic. Preview validates the entire file in a rolled-back transaction; confirmation revalidates and saves everything together or nothing. Existing records are never overwritten or deleted:

- Players match by case-insensitive name; decks match by owner and case-insensitive deck name. Existing settings are kept. A same-name deck with different commanders blocks the transfer; rename one first. Review names before combining unrelated playgroups.
- Games retain a stable portable UUID, so repeat transfers do not duplicate games, even if they were edited afterward. Original CSV/Mythic Track/Discord identities are also checked to recognize games already imported independently. Manual games recorded independently on both instances cannot be automatically recognized as the same game.
- Selected card metadata is restored locally without fetching Scryfall. Existing cached metadata is retained. Sheet receipts prevent unchanged sheet rows from being applied again after transfer.

All routes require admin access: `GET /api/exports/portable` downloads the document; `POST /api/imports/portable/preview` and `POST /api/imports/portable` accept `{"json":"<export file text>"}`. Confirmation also requires recent reauthentication. Files declare `format: "the-gathering"` and `version: 1`; unknown format versions are rejected rather than guessed.

## Native format

[Download a sample CSV](/api/imports/csv/sample), or use these columns:

| Column | Required | Meaning |
| --- | --- | --- |
| `game_id` | yes | A value that groups all seats from one game. It only needs to be unique within the file. |
| `date` | yes | ISO 8601 date/time or `YYYY-MM-DD`. A date without a time becomes noon UTC. |
| `player` | yes | Player name. Matching is case-insensitive; a missing player is created. |
| `deck` | yes | Deck name, unique for that player. An existing deck is reused when its name matches, or when the same player already has a deck with the same commander (and partner). Otherwise the deck is created. |
| `commander` | yes | Commander name used when creating the deck. |
| `seat` | yes | Consecutive number from 1 through the number of players. |
| `result` | yes | `win`, `loss`, or `draw`. A game has one winner and all other players lose, or every player draws. |
| `mvp_card` | no | MVP/key card for this seat. |
| `duration_minutes` | no | Positive whole number. Use the same value on every row for a game. |
| `turns` | no | Positive whole number. Use the same value on every row for a game. |
| `notes` | no | Game notes. Use the same value on every row for a game. |
| `kills` | no | Per-player total, 0–5. Zero is known zero; blank is unknown on creation and preserves the existing value on update. |
| `partner` | no | Partner commander name. |
| `win_condition` | no | One of the enum keys below; identical on every row for a game. |
| `action` | no | `create` (default), `update`, or `skip`. |
| `source`, `external_id` | for source-linked updates | The existing game's original identity, e.g. `mythic_track` and its exported GUID. Both are required together. |
| `portable_id` | alternative update identity | UUID from a portable JSON export. Never use an instance-local numeric game ID. |

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

## Correcting existing games

Use the **normal CSV importer** with `action=update` and either `source` + `external_id` or `portable_id`. This is an explicit correction to that game, independent of dates or participant names. `game_id` only groups rows within the file. If both identities are supplied, they must resolve to the same game. Missing targets, conflicting identities and two groups targeting one game block the batch; updates never fall back to creating a game.

Supply the complete corrected seat list and date. Preview shows actual before/after changes and separates creates, updates and unchanged games. Game IDs, source identities and portable IDs survive corrections. Retained players keep their seat IDs, seat notes, MVPs and elimination details; removing a participant removes their seat, and clears any remaining elimination attribution to them. Seat order can be corrected. Decks are reused by owner/name or commander pair; a same-name deck with conflicting commanders blocks the import instead of changing a shared deck. Give the replacement deck a different name.

Blank optional game/seat fields preserve existing values during updates; explicit `0` kills replaces the count. A supplied notes value replaces game notes, so include any old notes you want to retain. This format does not provide an explicit-clear operation. Dates without a time become noon UTC; full timestamps retain their supplied instant. When repairing a date-only historical record stored at midnight UTC, supply `YYYY-MM-DD` to avoid the usual previous-day display in American timezones. This is not a general timezone conversion.

Preview validates persistence in a rolled-back transaction. Confirmation revalidates atomically and requires the returned `revision` alongside `csv` when any group has `action=update`. A changed CSV or database invalidates that revision. Repeating an unchanged correction skips it without rewriting the game. Take a fresh export before repairing production history, especially if it has edits not present in the original source.

Win-condition keys: `damage`, `infinite_combo`, `mill`, `poison`, `alternate_win_con`, `hard_lock`, `commander_damage`, `draw`, `non_combat_damage`, `combat_damage`, `concede`, `unknown`. Leave blank when not recorded. Specific descriptions such as a card combo belong in notes; the category is not free text.

## Reconciling the original Google Sheet

The group-specific Google Sheet screen has been retired in favor of the generic CSV workflow above. Transform a sheet into seat-per-row CSV, resolving aliases and matching source IDs before upload. Do not guess missing opponents or commanders. The following describes the legacy sheet API, retained for compatibility.

It accepts the one-game-per-row sheet with `Date`, `Winner`, `Deck`, named kill columns, `Win Con`, `Other Decks`, and `Notes`. A decorative `K I L L S` row is allowed. Dates accept `M/D/YY` (2000–2099), `M/D/YYYY`, and ISO dates.

1. Read the sheet. The preview automatically matches games by mapped participants and date, preferring the exact UTC day and falling back to the preceding/following day. When multiple games qualify, a unique best deck-name/commander match can distinguish them; winners are not used to hide result discrepancies. Ambiguous matches and invalid rows stay skipped for review. Nothing is saved until confirmation.
2. Map player aliases to existing players. Mappings apply to both participants and kill-column headers. Missing players require an explicit create choice. Deck mappings apply to the same sheet player/deck pair throughout the batch.
3. Review actual **before → after** values. Matching games with changes are selected together; unchanged games are skipped. Filters separate result/deck corrections from kills/notes-only changes, unresolved rows, and unchanged games. Preserved fields and deck nicknames do not count as changes. All selected rows are confirmed, including rows hidden by a filter. You can skip any match, choose another game, or explicitly create a missing game. An update must have the same mapped player set. Repair differing participants in the sheet or game editor first.
4. Refresh the preview after changing any choice, then confirm selected changes. Invalid skipped rows do not block other rows. Invalid selected rows block the entire batch. Commits require admin and recent password authentication.

Updates retain game/seat IDs, original source/external ID, timestamps, turn order, duration, turns, MVPs and seat notes. Existing deck links are retained unless explicitly mapped. Sheet results and nonempty notes (including `Win con:`) replace existing values. Empty notes leave existing notes intact. No games outside the selected targets are removed or changed.

Kills are per-player totals, not victim assignments. In this Google Sheet, blank cells mean **zero**, including participants without a kill column. Both blank and explicit zero replace existing counts. Manually entered games can still leave kills unknown. Fewer kills than opponents are allowed (scoops and alternate wins). Impossible totals, alias collisions, malformed opponents and missing participants require repair. Blank/`N/A` winners produce draws for every listed player; prose notes never infer results.

New games use noon UTC and sheet ordering as placeholder seat order; the sheet does not record time or turn order. Missing decks need explicit mapping or creation; creation uses the sheet text as the deck and unverified commander name, without guessing card identity. Prefer mappings to cleaned-up decks.

Reconciliation receipts remember each imported row independently of the game's original source. Re-uploading the same rows skips them, even after subsequent game edits. Edited sheet content is a new row to review, not permission to overwrite automatically. Two rows cannot target one game in a batch. A database change after preview invalidates confirmation; refresh instead. Selected changes and receipts commit in one transaction or roll back together. Back up the database before reconciling production history.

The admin API is `POST /api/imports/sheet/preview` with `{text, players, decks, actions}`. `players` maps raw names to player IDs or `"new"`; `decks` maps JSON-encoded `[rawPlayer, rawDeck]` keys to deck IDs or `"new"`; `actions` maps preview row keys to existing game IDs, `"create"`, or `"skip"`. Commit to `POST /api/imports/sheet` with the same input plus the returned `revision`.
