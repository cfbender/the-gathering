# Deck chooser scripts

The deck chooser (**Decks → Choose a deck**) draws one of your playable decks at random,
weighted so some decks come up more often than others. Out of the box the weight favors
decks you have not played lately, decks you keep skipping, and decks you have played less.

If you want different rules, such as giving a deck a rematch after a quick loss, more color
variety, or faster decks late at night, write a small [Lua 5.4](https://www.lua.org/manual/5.4/)
script under **Choose a deck → Customize**. The editor previews every deck's chance under
your draft before you save it, shows the exact tables your script receives, and ships
[examples](#examples) to start from. Scripts belong to your account and only weigh your
own decks.

## The `weight` function

A script defines one global function:

```lua
function weight(deck, ctx)
  return deck.base_weight
end
```

The chooser calls `weight(deck, ctx)` once for each deck it can pick and draws a deck in
proportion to the numbers you return: a deck weighted 20 is twice as likely as one weighted
10. Each call gets the same `ctx`; `ctx.decks` lists every `deck` table, so you can compare
decks with each other.

- Return a number zero or above. `0` leaves a deck out. Negative numbers, `nan`, infinity,
  and anything other than a number are errors.
- Return a second value to explain the weight, such as `return w, "quick loss x3"`. The
  chooser shows this note on the pick, and the preview shows it next to each deck. Notes
  are cut at 80 characters.
- `deck.base_weight` is the built-in weight, so a script can adjust it instead of
  replacing it.
- `print(...)` writes to the preview's log (up to 200 lines), which is the easiest way to
  inspect values while you write.
- Code outside `weight` runs once per pick, before the calls. Use it for constants and
  helper functions.

When a deck was just skipped, the chooser leaves it out (if any other deck is left) and sets
`ctx.excluded_deck_id`.

## When a script fails

If the script has a syntax error or does not define `weight`, it cannot be saved. If it
fails during a pick (a runtime error, an invalid weight, `0` for every deck, or hitting a
limit), the chooser uses the built-in weights for that pick and shows the error with its
line number, so a broken script never blocks choosing a deck. The preview shows the same
error while you edit.

## Sandbox and limits

Scripts run on the server in a fresh Lua state for every pick and preview, with only the
`string`, `table`, `math`, and `utf8` libraries and the safe parts of the base library
(`pairs`, `ipairs`, `tostring`, `tonumber`, `type`, `select`, `pcall`, `error`, `assert`,
`setmetatable`, and so on). There is no `io`, `os`, `debug`, `package`, `require`, `load`,
`loadfile`, `dofile`, `collectgarbage`, or `string.dump`, so a script cannot read files,
run programs, or load other code. Use `ctx.now` for the time instead of `os.time` or
`os.date`.

Each run is limited to 32 MB of memory, 50 million Lua instructions, and 500 ms, and a
script can be up to 64 KB long. A deck chooser script is usually a few dozen lines that run
in well under a millisecond.

`math.random` works and is seeded differently on every run, so a script that uses it gives
a different preview each time.

## Field reference

Values that are unknown are `nil`; fields marked "or `nil`" can be missing, so check them
before doing arithmetic (`if deck.avg_turns then ... end`). Times are Unix seconds unless the
field is a [`time`](#time) table. Lists are Lua sequences (use `ipairs` and `#`), newest
first where they hold games.

### `deck`

One of your playable decks: not retired, and included in random picks.

| Field | Type | Description |
| --- | --- | --- |
| `deck.id` | integer | The deck's id. |
| `deck.name` | string | The deck's name. |
| `deck.commander` | string | The commander's name. |
| `deck.partner` | string or `nil` | The partner or background's name. |
| `deck.color_identity` | string | Color identity letters in WUBRG order, such as `"UR"`; `""` for colorless. |
| `deck.colors` | list of strings | The same letters as a list, such as `{"U", "R"}`. |
| `deck.color_set` | table | The letters as keys, such as `{U = true, R = true}`, for `if deck.color_set.G then`. |
| `deck.color_count` | integer | How many colors, 0 to 5. |
| `deck.color_name` | string | The combination's name, such as `"Izzet"`, `"Mono-Green"`, or `"Colorless"`. |
| `deck.colorless` | boolean | Whether the color identity is empty. |
| `deck.decklist_source` | string or `nil` | Where the deck list lives: `"moxfield"`, `"archidekt"`, `"manavault"`, or `"other"`. |
| `deck.added_at` | integer | When the deck was added. |
| `deck.days_since_added` | number | Days since the deck was added. |
| `deck.plays` | integer | Games logged with this deck. |
| `deck.skips` | integer | Times the chooser offered this deck and you skipped it since you last chose it. |
| `deck.wins` | integer | Games won with this deck. |
| `deck.losses` | integer | Games lost with this deck. |
| `deck.draws` | integer | Games drawn with this deck. |
| `deck.win_rate` | number or `nil` | Wins divided by games, from 0 to 1; `nil` before its first game. |
| `deck.base_weight` | number | The built-in weight: hours since last played × (skips + 1) ÷ (plays + 1), where never-played decks count as a month older than your longest-unplayed deck. |
| `deck.never_played` | boolean | Whether no game was logged with this deck. |
| `deck.last_played_at` | integer or `nil` | When the deck was last played. |
| `deck.hours_since_played` | number or `nil` | Hours since the deck was last played. |
| `deck.days_since_played` | number or `nil` | Days since the deck was last played. |
| `deck.plays_today` | integer | Games with this deck since local midnight. |
| `deck.plays_this_session` | integer | Games with this deck in the current session (see `ctx.session`). |
| `deck.plays_last_7_days` | integer | Games with this deck in the last 7 days. |
| `deck.plays_last_30_days` | integer | Games with this deck in the last 30 days. |
| `deck.avg_duration_minutes` | number or `nil` | Average length of its games that logged a duration. |
| `deck.avg_turns` | number or `nil` | Average turns of its games that logged turns. |
| `deck.streak` | table or `nil` | The deck's current run of identical results; `nil` before its first game. |
| `deck.streak.result` | string | `"win"`, `"loss"`, or `"draw"`. |
| `deck.streak.length` | integer | How many of its latest games in a row had that result. |
| `deck.last_game` | [`game`](#game) or `nil` | The deck's latest game. |
| `deck.games` | list of [`game`](#game) | The deck's latest 25 games, newest first. |

### `game`

A game you played, from your seat. `deck.games`, `deck.last_game`, `ctx.recent_games`,
`ctx.today`, `ctx.session`, and `ctx.last_game` hold these.

| Field | Type | Description |
| --- | --- | --- |
| `game.id` | integer | The game's id. |
| `game.played_at` | integer | When the game was played. |
| `game.local` | [`time`](#time) | When the game was played, in your time zone. |
| `game.hours_ago` | number | Hours since the game. |
| `game.days_ago` | number | Days since the game. |
| `game.today` | boolean | Whether it was played since local midnight. |
| `game.in_session` | boolean | Whether it is part of the current session (see `ctx.session`). |
| `game.format` | string | `"commander"`, `"two_headed_giant"`, or `"five_star"`. |
| `game.win_condition` | string or `nil` | How it was won, such as `"combat_damage"`, `"commander_damage"`, `"infinite_combo"`, `"poison"`, `"mill"`, `"alternate_win_con"`, `"hard_lock"`, `"non_combat_damage"`, `"damage"`, `"concede"`, `"draw"`, or `"unknown"`. |
| `game.duration_minutes` | integer or `nil` | How long the game took, when logged. |
| `game.turns` | integer or `nil` | How many turns it took, when logged. |
| `game.players` | integer | Seats at the table, including yours. |
| `game.result` | string | Your result: `"win"`, `"loss"`, or `"draw"`. |
| `game.won` | boolean | Whether you won. |
| `game.lost` | boolean | Whether you lost. |
| `game.drew` | boolean | Whether you drew. |
| `game.deck_id` | integer or `nil` | The deck you played; `nil` when none was logged. |
| `game.deck_name` | string or `nil` | Its name. |
| `game.commander` | string or `nil` | Its commander. |
| `game.partner` | string or `nil` | Its partner. |
| `game.color_identity` | string | Its color identity in WUBRG order (`""` without a deck). |
| `game.colors` | list of strings | Its colors as a list. |
| `game.color_set` | table | Its colors as keys. |
| `game.color_count` | integer | How many colors it has. |
| `game.color_name` | string | The combination's name. |
| `game.eliminated_turn` | integer or `nil` | The turn you were knocked out, when logged. |
| `game.eliminated_by` | string or `nil` | The player who knocked you out, when logged. |
| `game.kills` | integer or `nil` | Players you knocked out, when logged. |
| `game.mvp_card` | string or `nil` | Your MVP card, when logged. |
| `game.winners` | list of strings | The names of the players who won (empty for a draw). |
| `game.opponents` | list of [`opponent`](#opponent) | Everyone else at the table, in seat order. |

### `opponent`

Another seat in a `game`.

| Field | Type | Description |
| --- | --- | --- |
| `opponent.player_id` | integer | The player's id. |
| `opponent.name` | string | The player's name. |
| `opponent.deck_id` | integer or `nil` | The deck they played, when logged. |
| `opponent.deck_name` | string or `nil` | Its name. |
| `opponent.commander` | string or `nil` | Its commander. |
| `opponent.partner` | string or `nil` | Its partner. |
| `opponent.color_identity` | string | Its color identity in WUBRG order (`""` without a deck). |
| `opponent.colors` | list of strings | Its colors as a list. |
| `opponent.color_set` | table | Its colors as keys. |
| `opponent.color_count` | integer | How many colors it has. |
| `opponent.color_name` | string | The combination's name. |
| `opponent.result` | string | Their result: `"win"`, `"loss"`, or `"draw"`. |
| `opponent.won` | boolean | Whether they won. |
| `opponent.kills` | integer or `nil` | Players they knocked out, when logged. |
| `opponent.eliminated_turn` | integer or `nil` | The turn they were knocked out, when logged. |

### `time`

A moment in your time zone: the browser's zone when you open the chooser, or UTC when it
is unknown. `ctx.now` and `game.local` are `time` tables.

| Field | Type | Description |
| --- | --- | --- |
| `time.unix` | integer | Unix seconds. |
| `time.date` | string | The local date, such as `"2026-10-16"`. |
| `time.year` | integer | The local year. |
| `time.month` | integer | The local month, 1 to 12. |
| `time.day` | integer | The local day of the month. |
| `time.hour` | integer | The local hour, 0 to 23. |
| `time.minute` | integer | The local minute, 0 to 59. |
| `time.weekday` | integer | The day of the week, 1 (Sunday) to 7 (Saturday), like Lua's `os.date("*t").wday`. |
| `time.weekday_name` | string | The lowercase day name, such as `"friday"`. |
| `time.weekend` | boolean | Whether it is Saturday or Sunday. |

### `ctx`

What the chooser knows about right now and your history. All game lists are newest first.

| Field | Type | Description |
| --- | --- | --- |
| `ctx.now` | [`time`](#time) | The current time in your time zone. |
| `ctx.timezone` | string | The IANA zone name, such as `"America/New_York"` (`"Etc/UTC"` when unknown). |
| `ctx.player` | table | Your linked player. |
| `ctx.player.id` | integer | Its id. |
| `ctx.player.name` | string | Its name. |
| `ctx.excluded_deck_id` | integer or `nil` | The deck you just skipped, which is not among `ctx.decks`. |
| `ctx.games_today` | integer | Games you played since local midnight. |
| `ctx.session_games` | integer | Games in the current session; the same as `#ctx.session`. |
| `ctx.minutes_since_last_game` | number or `nil` | Minutes since your latest game. |
| `ctx.last_game` | [`game`](#game) or `nil` | Your latest game. |
| `ctx.today` | list of [`game`](#game) | Your games since local midnight. |
| `ctx.session` | list of [`game`](#game) | Your games tonight: the latest game if it was within 4 hours, and each earlier game within 4 hours of the next one. Unlike `ctx.today`, a session that runs past midnight stays one session. |
| `ctx.recent_games` | list of [`game`](#game) | Your latest 100 games, with any deck. |
| `ctx.recent_colors` | table | How many of your last 10 games used each color: `{W = 2, U = 0, B = 1, R = 3, G = 0, C = 1}`, where `C` counts colorless decks. |
| `ctx.stats` | table | Your all-time record. |
| `ctx.stats.games` | integer | Games you played. |
| `ctx.stats.wins` | integer | Games you won. |
| `ctx.stats.losses` | integer | Games you lost. |
| `ctx.stats.draws` | integer | Games you drew. |
| `ctx.stats.win_rate` | number or `nil` | Wins divided by games, from 0 to 1. |
| `ctx.stats.decks` | integer | How many decks the chooser is weighing (`#ctx.decks`). |
| `ctx.decks` | list of [`deck`](#deck) | Every deck being weighed, in the order `weight` is called. |

## Examples

The editor's **Examples** menu loads these scripts, which live in
[`assets/react/src/features/decks/deck-scripts/`](../assets/react/src/features/decks/deck-scripts/):

- **Starter** (`default.lua`): the built-in weights with comments to build on.
- **Quick-loss rematch** (`quick-loss-rematch.lua`): boosts a deck whose last game was a fast
  loss, fading over two weeks.
- **Color variety** (`color-variety.lua`): favors colors you have not played in your last 10 games.
- **Time of night** (`time-of-night.lua`): faster decks late at night, new decks on weekends,
  winners on Fridays.
- **Session pacing** (`session-pacing.lua`): open the night with a strong deck, avoid repeats,
  and favor shorter games later.
- **Revenge** (`revenge.lua`): if someone beat you last game, favor decks that have beaten them.
- **New and neglected** (`new-and-neglected.lua`): boosts barely played decks and decks unplayed this month.
- **Balance win rates** (`balance-win-rates.lua`): more table time for struggling decks, less
  for decks on a winning streak.
- **Your own formula** (`your-own-formula.lua`): ignores the built-in weight and starts from scratch.
- **All together** (`all-together.lua`): several rules as multipliers, each explained in the note.

## API

- `GET /api/deck-chooser/script` returns `{"data": {"script": "..." | null}}`.
- `PUT /api/deck-chooser/script` with `{"script": "..." | null}` saves the script, or
  restores the built-in weights for `null` or a blank script. A script that does not compile
  or does not define `weight` fails with `422` and `{"errors": {"script": ["line 3: ..."]}}`.
- `POST /api/deck-chooser/preview` with `{"script": "..." | null, "tz": "America/New_York",
  "exclude_id": 12}` weighs your decks with a draft script without saving it or picking. It
  returns `source` (`"script"` or `"built_in"`), `error` (`{"message", "line"}` or `null`),
  `logs`, `context` (the `ctx` table without `decks`), and `decks`, where each entry has the
  deck's `id`, `name`, `base_weight`, final `weight`, `chance` (0 to 1), `note`, and `input`
  (the `deck` table the script received).
- `GET /api/deck-chooser?tz=America/New_York` picks with your saved script and adds
  `chance`, `note`, and `script_error` (non-null when it fell back to the built-in weights)
  to the pick.
