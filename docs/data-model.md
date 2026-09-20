# Game tracking data model

The game tracker is owned by `TheGathering.Games`. One installation currently represents one playgroup; no table or query assumes a synthetic playgroup ID, so a future `playgroup_id` can be added to players/games and their uniqueness scopes.

## Tables

### `players`

| Column | Notes |
| --- | --- |
| `name` | Required; unique with SQLite `NOCASE` collation. |
| `user_id` | Nullable, unique foreign key to `users`; deletes are restricted. |
| `discord_id` | Nullable, unique string used for importer matching. |
| `archived_at` | Nullable UTC timestamp; archived players are hidden from normal lists. |

A player does not require a user account, which supports guests and historical imports.
Member API requests may set only guest-editable fields such as `name` and `archived_at`;
`user_id` and `discord_id` are assigned only by trusted account, OAuth, import, and bot paths.

### `decks`

Each deck belongs to a player. `name` is case-insensitively unique within that player. `commander_card_id` and `partner_card_id` hold Scryfall UUIDs without foreign keys until the catalog integration lands; the corresponding required `commander_name` and optional `partner_name` are durable display snapshots. `color_identity` is a compact, validated WUBRG string (for example `WUG` or the empty string), rather than catalog-derived JSON.

`decklist_source` is derived from `decklist_url` as `moxfield`, `archidekt`, `manavault`, or `other`. `archived_at` has the same list semantics as players.

Members may update or delete only decks owned by their linked player, and administrators may
manage any deck. A deck's owner cannot be changed through the API update operation. Creating a
deck for an unclaimed guest remains supported while logging a game.

### `games`

| Column | Notes |
| --- | --- |
| `played_at` | Required UTC datetime; lists order by this descending, then ID. |
| `duration_minutes`, `turns` | Nullable positive integers. |
| `notes` | Nullable text. |
| `source` | `manual`, `csv`, `mythic_track`, or `discord`. |
| `external_id` | Nullable; unique together with `source` for idempotent imports. |
| `created_by_user_id` | Nullable foreign key to the user that created/imported the game; deletes are restricted. |

Game updates and deletion are allowed for administrators, the creating user, and users whose
linked player has a seat in the game. Other members receive `403 Forbidden`. Member create and
update payloads cannot set `source` or `external_id`; import and Discord ingestion assign those
provenance fields through trusted context operations.

### `game_players`

This is a seat/result fact, not a generic join table. It stores the game's 1-based turn order, player, optional deck, `win`/`loss`/`draw` result, optional elimination facts, optional Scryfall MVP UUID plus name snapshot, and notes. Player and seat are each unique per game. A game must have two through six consecutive seats and either exactly one winner or all draws. The context also verifies that every selected deck belongs to the seat's player.

These normalized rows make player, deck, commander, matchup, streak, duration, and turn statistics queryable without decoding JSON.

Users are soft-disabled rather than deleted. Both user foreign keys therefore use `ON DELETE RESTRICT` so an accidental hard delete cannot silently erase player linkage or game authorship.

## Context contract for importers

CSV and Mythic Track parsers normalize source payloads into typed
`TheGathering.Imports.Game` and `TheGathering.Imports.Seat` structs. `Imports.Preview`
resolves the proposed player/deck plan without writing. `Imports.Commit` parses and
previews before opening a transaction, then re-resolves players, decks, and external game
identities while writing the batch atomically. After commit, only the imported games are
passed to `Games.LinkCatalogCards`; global historical repair remains an explicit bounded
catalog backfill operation.

- `resolve_player(name, discord_id, opts \\ [])` treats a supplied Discord ID as authoritative:
  it matches only that identity and otherwise creates a player with an available suffixed name.
  Name matching is used only when the incoming identity has no Discord ID. Import preview uses
  the same resolver policy and reports the distinct name that commit will create. Discord OAuth
  and bot ingestion also use this policy; resolver changeset and account-link conflicts are
  returned to callers rather than ignored.
- `find_or_create_player_by_name(name, attrs \\ %{})` matches names case-insensitively.
- `find_or_create_player_by_discord_id(discord_id, name)` is a compatibility wrapper around
  `resolve_player/3`.
- `find_or_create_deck(player_or_id, name, attrs \\ %{})` matches deck names case-insensitively within one owner. New decks require `commander_name` in `attrs`.
- `find_or_create_game_by_external_id(source, external_id, attrs)` and `create_game/1` are idempotent when both external identity fields are present; a repeat returns the existing, fully preloaded game.
- `create_game/1` and `update_game/2` accept nested `seats` and persist the game atomically.
- `list_games/1` accepts `player_id`, `deck_id`, `date_from`, `date_to`, `page`, and `per_page` (capped at 100).
- `get_game!/1` preloads each seat's player and deck.

The public `Games` context remains the compatibility boundary. Complete workflows are
owned by `Games.RecordGame` (nested game/seat writes), `Games.MergePlayers` (all player,
deck, seat, and elimination references), and `Games.LinkCatalogCards` (Deck/GamePlayer
catalog links). Card-name lookup belongs to `Catalog.find_card_by_name/1`.

Importer order should be player → deck → game. Keep raw source payloads outside these tables if audit storage is later needed; game facts themselves stay relational.

## JSON API

Resources are available at `/api/players`, `/api/decks`, and `/api/games`; each supports index, create, show, update, and delete. Deck index accepts `player_id`. Game index accepts the filters above and returns pagination metadata.

Create or update a game with nested seats:

```json
{
  "game": {
    "played_at": "2026-09-19T22:30:00Z",
    "duration_minutes": 57,
    "turns": 9,
    "notes": "Combat damage",
    "seats": [
      {
        "player_id": 1,
        "deck_id": 4,
        "seat": 1,
        "result": "win",
        "mvp_card_name": "Swan Song"
      },
      { "player_id": 2, "deck_id": 7, "seat": 2, "result": "loss" }
    ]
  }
}
```

Responses use the standard envelope. Game payloads include ordered seats with nested player and deck summaries:

```json
{
  "data": {
    "id": 42,
    "played_at": "2026-09-19T22:30:00Z",
    "source": "manual",
    "seats": [
      {
        "id": 91,
        "seat": 1,
        "result": "win",
        "player": { "id": 1, "name": "Alice", "archived_at": null },
        "deck": { "id": 4, "name": "Birds", "commander_name": "Kangee, Sky Warden" }
      }
    ]
  }
}
```

`GET /api/games?page=2&per_page=20&player_id=1` also includes `"pagination": {"page": 2, "per_page": 20, "total": 37, "total_pages": 2}`.
