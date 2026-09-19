# Game tracking data model

The game tracker is owned by `TheGathering.Games`. One installation currently represents one playgroup; no table or query assumes a synthetic playgroup ID, so a future `playgroup_id` can be added to players/games and their uniqueness scopes.

## Tables

### `players`

| Column | Notes |
| --- | --- |
| `name` | Required; unique with SQLite `NOCASE` collation. |
| `user_id` | Nullable, unique integer. Intentionally has no foreign key until `TheGathering.Accounts` lands. |
| `discord_id` | Nullable, unique string used for importer matching. |
| `archived_at` | Nullable UTC timestamp; archived players are hidden from normal lists. |

A player does not require a user account, which supports guests and historical imports.

### `decks`

Each deck belongs to a player. `name` is case-insensitively unique within that player. `commander_card_id` and `partner_card_id` hold Scryfall UUIDs without foreign keys until the catalog integration lands; the corresponding required `commander_name` and optional `partner_name` are durable display snapshots. `color_identity` is a compact, validated WUBRG string (for example `WUG` or the empty string), rather than catalog-derived JSON.

`decklist_source` is derived from `decklist_url` as `moxfield`, `archidekt`, `manavault`, or `other`. `archived_at` has the same list semantics as players.

### `games`

| Column | Notes |
| --- | --- |
| `played_at` | Required UTC datetime; lists order by this descending, then ID. |
| `duration_minutes`, `turns` | Nullable positive integers. |
| `notes` | Nullable text. |
| `source` | `manual`, `csv`, or `discord`. |
| `external_id` | Nullable; unique together with `source` for idempotent imports. |
| `created_by_user_id` | Nullable integer with no FK until auth integration. |

### `game_players`

This is a seat/result fact, not a generic join table. It stores the game's 1-based turn order, player, optional deck, `win`/`loss`/`draw` result, optional elimination facts, optional Scryfall MVP UUID plus name snapshot, and notes. Player and seat are each unique per game. A game must have two through six consecutive seats and either exactly one winner or all draws. The context also verifies that every selected deck belongs to the seat's player.

These normalized rows make player, deck, commander, matchup, streak, duration, and turn statistics queryable without decoding JSON.

## Context contract for importers

- `find_or_create_player_by_name(name, attrs \\ %{})` matches names case-insensitively.
- `find_or_create_player_by_discord_id(discord_id, name)` matches stable Discord identity first.
- `find_or_create_deck(player_or_id, name, attrs \\ %{})` matches deck names case-insensitively within one owner. New decks require `commander_name` in `attrs`.
- `find_or_create_game_by_external_id(source, external_id, attrs)` and `create_game/1` are idempotent when both external identity fields are present; a repeat returns the existing, fully preloaded game.
- `create_game/1` and `update_game/2` accept nested `seats` and persist the game atomically.
- `list_games/1` accepts `player_id`, `deck_id`, `date_from`, `date_to`, `page`, and `per_page` (capped at 100).
- `get_game!/1` preloads each seat's player and deck.

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
    "source": "manual",
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
