# Statistics API

All statistics routes require an authenticated session and return `{ "data": ... }`. Each accepts optional inclusive `date_from` and `date_to` query parameters in `YYYY-MM-DD` format. Historical rows remain included when a player or deck is archived.

## `GET /api/stats/overview`

Returns playgroup `games_count`, average duration and turns, `leaderboard`, monthly game counts, seat and color-identity win rates, most-played commanders, and recent games. Record rows contain `games`, `wins`, `losses`, `draws`, and `win_rate` (a percentage from 0–100).

## `GET /api/stats/players/:id`

Returns the player identity, overall record, current and longest win streaks, ten-game recent form, cumulative win rate over time, deck records, head-to-head records, seat performance, favorite/best seat, and most-mentioned MVP cards.

## `GET /api/stats/decks/:id`

Returns deck and owner identity, overall record, average duration and turns, opponents faced, cumulative win rate over time, and recent games with the deck's result.
