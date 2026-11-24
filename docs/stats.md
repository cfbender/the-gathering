# Statistics API

All statistics routes require a signed-in session, cover the active playgroup, and return
`{"data": ...}`. Every route accepts the same optional query parameters:

| Parameter | Description |
| --- | --- |
| `date_from` | First day to include, `YYYY-MM-DD`. |
| `date_to` | Last day to include, `YYYY-MM-DD`. |
| `tz` | IANA time zone the days are read in. Defaults to UTC; the app sends the browser's zone. |

A date that does not parse is ignored. Archived players and retired decks stay in the
figures for games they played.

Record rows carry `games`, `wins`, `losses`, `draws`, and `win_rate` (a percentage from 0 to
100).

## Detailed-statistics cutoff

An administrator can set **Detailed statistics from** under **Admin → Settings** (stored in
`server_settings.detailed_stats_from`). The overview, player, and deck payloads echo it as
`detailed_stats_from` (`"YYYY-MM-DD"` or `null`).

Win/loss/draw figures always use every game. Figures built from data a playgroup may have
started recording later use only games played on or after the cutoff: seat win rates,
favorite and best seat, average duration and turns, game lengths, and MVP cards. A `null`
cutoff uses every game.

## `GET /api/stats/overview`

The playgroup's `games_count`, `leaderboard`, `elo` ratings, `matchups`, `kills`,
`win_conditions`, average duration and turns, `game_lengths`, `game_times`, `games_by_month`,
`seat_win_rates` (four-player games only), `color_win_rates`, `color_exposure`, the eight
most-played `commanders`, and the six most recent games. Elo ratings carry in from games
before `date_from`.

## `GET /api/stats/players/:id`

The `player`, overall `record`, `elo` rating and rank, win and loss conditions, average
duration and turns, `game_lengths`, current win and loss and longest win `streaks`, `recent_form` (the last
ten results), cumulative `win_rate_over_time`, deck records (retired decks are marked
`retired: true`), `head_to_head` records with kills, `rival_commanders`, color records, seat
win rates, `favorite_seat` and `best_seat`, and the most-mentioned `mvp_cards`. Responds 404
for an unknown player.

## `GET /api/stats/decks/:id`

The `deck` and its owner, overall `record`, average duration and turns, `opponents` faced,
cumulative `win_rate_over_time`, and recent games with the deck's result. Responds 404 for an
unknown deck.

## `GET /api/stats/commanders`

Every commander played, most played first. Each row has the commander's `id` (its Scryfall
card ID, or the card name when the catalog lacks the card), `name`, card image URLs,
`color_identity`, the record fields, distinct `pilots` and `decks` counts, and
`last_played_at`. A seat counts once for each commander its deck ran, so a partner deck
counts toward both partners. Decks recorded with only a commander name group by normalized
name.

## `GET /api/stats/commanders/:id`

Takes a Scryfall card ID or a card name. Returns the `commander`, overall `record`, per-pilot
and per-deck records, `partners` it was paired with, `opponents` faced, cumulative
`win_rate_over_time`, and `recent_games` with the commander's result. Responds 404 when the
commander was never played in the range.
