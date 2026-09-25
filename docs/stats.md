# Statistics API

All statistics routes require an authenticated session and return `{ "data": ... }`. Each accepts optional inclusive `date_from` and `date_to` query parameters in `YYYY-MM-DD` format, read as calendar days in the optional `tz` IANA zone (default UTC; the frontend sends the browser's zone). Historical rows remain included when a player or deck is archived.

## Detailed-statistics cutoff

An administrator can set **Detailed statistics from** under **Admin → Users** (stored as the nullable `server_settings.detailed_stats_from` date). Every payload echoes it as `detailed_stats_from` (`"YYYY-MM-DD"` or `null`).

Games played before that date still count toward every win/loss/draw record: `games_count`, the leaderboard, player records, streaks, form, win rate over time, head-to-head, deck records, commander and color-identity records. Only the figures whose inputs a pod may not have recorded early on are limited to games played on or after the cutoff: seat win rates, favorite and best seat, average duration, average turns, and MVP cards. A `null` cutoff uses every game.

## `GET /api/stats/overview`

Returns playgroup `games_count`, average duration and turns, `leaderboard`, monthly game counts, seat and color-identity win rates, most-played commanders, and recent games. Record rows contain `games`, `wins`, `losses`, `draws`, and `win_rate` (a percentage from 0–100).

## `GET /api/stats/players/:id`

Returns the player identity, overall record, current and longest win streaks, ten-game recent form, cumulative win rate over time, deck records, head-to-head records, seat performance, favorite/best seat, and most-mentioned MVP cards.

## `GET /api/stats/decks/:id`

Returns deck and owner identity, overall record, average duration and turns, opponents faced, cumulative win rate over time, and recent games with the deck's result.

## `GET /api/stats/commanders`

Returns every commander played across the playgroup, most played first. Each row carries the commander's `id` (its Scryfall card ID, or the card name when the card is missing from the catalog), `name`, `art_crop_url`, canonical `color_identity`, the record fields, distinct `pilots` and `decks` counts, and `last_played_at`. A seat counts once for each commander card its deck ran, so a partner deck contributes to both partners. Decks that only recorded a commander name (older imports) are grouped by normalized name.

## `GET /api/stats/commanders/:id`

Accepts a Scryfall card ID or a card name. Returns the `commander` identity, overall `record`, per-pilot and per-deck records, `partners` it was paired with, `opponents` faced, cumulative `win_rate_over_time`, and `recent_games` with the commander's result. Responds 404 when the commander has never been played.

All stats endpoints accept optional `date_from` / `date_to` (inclusive ISO dates) and `tz`.
