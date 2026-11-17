# Game tracking data model

Players, decks, and games live in the server's `games` module
(`rust/crates/the-gathering/src/games`). `rust/schema.sql` is the full schema.

## Playgroups

Every player and game belongs to one playgroup (`playgroup_id`), and decks follow their
player. Player names, Discord IDs, linked accounts, and game source identities are unique
within a playgroup. Accounts join any number of playgroups through `playgroup_members` and
view one at a time (`users.playgroup_id`); every `/api` request reads and writes that
playgroup. An administrator can move a game to another playgroup
(`POST /api/admin/games/:id/move`), which finds or creates matching players and decks there.

## Tables

### `players`

| Column | Notes |
| --- | --- |
| `name` | Required; unique per playgroup, case-insensitively. |
| `user_id` | Nullable link to `users`, unique per playgroup; deletes are restricted. |
| `discord_id` | Nullable, unique per playgroup; used to match Discord and Mythic Track identities. |
| `archived_at` | Nullable UTC timestamp; archived players are hidden from normal lists. |

A player does not need an account, so guests and imported players are plain rows. Members
can set only `name` and `archived_at`. Accounts and Discord IDs are linked by sign-in,
imports, the Discord bot, and administrators. A player can be deleted only when it has no
decks or seats.

### `decks`

Each deck belongs to a player, and `name` is unique per player, case-insensitively.

| Column | Notes |
| --- | --- |
| `commander_name`, `partner_name` | Display names; the commander is required. |
| `commander_card_id`, `partner_card_id` | Nullable Scryfall card IDs, without foreign keys, so a deck keeps its card when the catalog lacks it. |
| `commander_printing_id`, `partner_printing_id` | Nullable chosen printings, foreign keys to `card_printings`. |
| `color_identity` | WUBRG letters such as `WUG`, or the empty string. |
| `decklist_url`, `decklist_source` | The link, and `moxfield`, `archidekt`, `manavault`, or `other` derived from its host. |
| `skip_count`, `included_for_play` | Deck chooser state: skips since the deck was last chosen, and whether the chooser may pick it. |
| `archived_at` | Set when the deck is retired. |

A retired deck is left out of `GET /api/decks` and the deck chooser. It still appears in its
player's `decks`, under "Retired decks", and its rows in player statistics carry
`retired: true`. Owners retire or restore a deck by `PATCH`ing `archived_at` to a timestamp
or `null`.

Members can update or delete only decks of their linked player; administrators can manage any
deck. An update cannot change the owner. Deleting a deck with `replacement_deck_id` moves its
seats to another of the player's decks; without it those seats keep no deck.

### `games`

| Column | Notes |
| --- | --- |
| `played_at` | Required UTC timestamp; lists sort by it descending, then by ID. |
| `format` | `commander` (default), `two_headed_giant`, `five_star`, `pauper_commander`, or `duel_commander`. |
| `duration_minutes`, `turns` | Nullable positive integers. |
| `win_condition` | Nullable key such as `combat_damage` or `concede` (`games/win_condition.rs`). |
| `notes` | Nullable text. |
| `source` | `manual`, `csv`, `mythic_track`, or `discord`. |
| `external_id` | Nullable; unique with `playgroup_id` and `source`, so imports are idempotent. |
| `portable_id` | Nullable UUID kept across portable transfers between servers. |
| `created_by_user_id` | Nullable link to the user who created or imported the game; deletes are restricted. |

Administrators, the game's creator, and members whose linked player has a seat can update or
delete a game; others get `403 Forbidden`. Request bodies cannot set `source` or
`external_id`; imports and the Discord bot set them.

### `game_players`

One row per seat: the 1-based `seat` (turn order), `player_id`, optional `deck_id`, `result`
(`win`, `loss`, or `draw`), optional `kills` (0–9), `eliminated_turn` and
`eliminated_by_player_id`, an optional MVP card (`mvp_card_id` and `mvp_card_name`), and
`notes`. Player and seat are each unique per game.

A game has 2–10 seats numbered 1 through n, and no player twice. It has exactly one winner
(two in Two-Headed Giant) with everyone else losing, or every seat draws. Each deck must
belong to its seat's player, a killer must be another player in the game, and a kill total
cannot be lower than the number of eliminations attributed to that player.

Users are disabled rather than deleted, and both user foreign keys use `ON DELETE RESTRICT`,
so a hard delete cannot erase player links or game authorship.

## Operations

`Games` (`games/mod.rs`) is the pool-level API; each write runs in its own transaction. The
submodules expose the same operations on a connection, so imports and the Discord bot can
compose them inside one transaction:

- `resolve_player::run`: a supplied Discord ID is authoritative. It matches only a player with
  that ID, and otherwise creates one, adding a suffix such as `Alice (2)` when the name is
  taken. Without a Discord ID it matches the name case-insensitively or creates the player.
  Import previews, Discord sign-in, and the bot use the same rules.
- `player::find_or_create_player_by_name`: case-insensitive name match.
- `deck::find_or_create_deck`: the player's deck by case-insensitive name, else by the same
  commander and partner in either order, else a new deck (which needs `commander_name`).
- `record_game`: `create`, `update`, and `delete` write a game and its seats together.
  `find_or_create_by_external_id` returns the existing game for a repeated
  `(source, external_id)`; `upsert_by_external_id` replaces it.
- `merge_players::merge_unseated`: moves every seat, deck, and account or Discord link to
  the target player and deletes the source. It refuses when both players sat in the same game
  or have different accounts or Discord IDs.
- `link_catalog_cards`: fills missing commander, partner, and MVP card IDs from catalog names.
  Imports run `link_game` on the games they create; the catalog backfill
  (`catalog/backfill.rs`) runs `repair_batch` over the whole history.
- `list_games::list_games`: the filtered, paginated history behind `GET /api/games`.

## JSON API

`/api/players`, `/api/decks`, and `/api/games` support index, create, show, update, and
delete. Indexes cover the active playgroup. `GET /api/players` and `GET /api/decks` accept
`include_archived=true`, and `GET /api/decks` accepts `player_id`.

`GET /api/games` takes `page` and `per_page` (default 20, at most 100) and these filters:

- Seats: `player_id` (with optional `player_result`), `winner_id`, `winner_seat`,
  `opponent_id`, `opponent_commander` and `opponent_result`, `killer_id` and `victim_id`,
  `deck_id`.
- Decks: `commander` (matches commander or partner), `colors` and `winner_colors` (exact
  identity as WUBRG letters, `C` for colorless), `color` and `winner_color` (identity includes
  that color).
- Games: `win_condition`, `player_count`, `min_turns`, `max_turns`, `min_duration`,
  `max_duration`.
- Time: `date_from` and `date_to` (inclusive days), `weekday` (0 = Sunday), and `hour`
  (0–23), all read in the `tz` IANA zone (default UTC).

When `player_id` is set, deck filters describe that player's seat, so "Alice with Golgari"
means Alice played Golgari. Winner filters all describe the same winning seat. Out-of-range
numbers such as `hour=25` are ignored.

Create or update a game with a flat body. On update, a given `seats` list replaces the
game's seats:

```json
{
  "played_at": "2026-09-19T22:30:00Z",
  "duration_minutes": 57,
  "turns": 9,
  "win_condition": "combat_damage",
  "notes": "Combat damage",
  "seats": [
    { "player_id": 1, "deck_id": 4, "seat": 1, "result": "win", "mvp_card_name": "Swan Song" },
    { "player_id": 2, "deck_id": 7, "seat": 2, "result": "loss" }
  ]
}
```

Game payloads list seats in order, each with nested player and deck summaries:

```json
{
  "data": {
    "id": 42,
    "played_at": "2026-09-19T22:30:00Z",
    "format": "commander",
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

`GET /api/games` adds
`"pagination": {"page": 2, "per_page": 20, "total": 37, "total_pages": 2}`.
