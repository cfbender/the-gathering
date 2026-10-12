-- Migration 20261012050542: backfill_kill_attribution.
-- Only infer a sole winner's sweep, and never contradict recorded attribution.
WITH sweeps AS (
    SELECT winner.game_id, winner.player_id
    FROM game_players AS winner
    WHERE winner.result = 'win'
      AND winner.kills = (SELECT COUNT(*) - 1 FROM game_players WHERE game_id = winner.game_id)
      AND winner.kills > 0
      AND (SELECT COUNT(*) FROM game_players WHERE game_id = winner.game_id AND result = 'win') = 1
      AND NOT EXISTS (
          SELECT 1 FROM game_players AS victim
          WHERE victim.game_id = winner.game_id
            AND victim.eliminated_by_player_id IS NOT NULL
            AND (victim.eliminated_by_player_id != winner.player_id OR victim.player_id = winner.player_id)
      )
)
UPDATE game_players AS victim
SET eliminated_by_player_id = sweeps.player_id
FROM sweeps
WHERE victim.game_id = sweeps.game_id
  AND victim.player_id != sweeps.player_id
  AND victim.eliminated_by_player_id IS NULL;
