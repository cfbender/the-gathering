-- Migration 20260924221950: sync player names to display names.

UPDATE players
SET name = (SELECT trim(u.display_name) FROM users u WHERE u.id = players.user_id),
    updated_at = CURRENT_TIMESTAMP
WHERE user_id IS NOT NULL
  AND EXISTS (
    SELECT 1 FROM users u
    WHERE u.id = players.user_id
      AND trim(u.display_name) <> ''
      AND trim(u.display_name) <> players.name
      AND NOT EXISTS (
        SELECT 1 FROM players other
        WHERE other.id <> players.id
          AND other.name = trim(u.display_name) COLLATE NOCASE
      )
      AND NOT EXISTS (
        SELECT 1 FROM users twin
        JOIN players twin_player ON twin_player.user_id = twin.id
        WHERE twin.id <> u.id
          AND trim(twin.display_name) = trim(u.display_name) COLLATE NOCASE
      )
  );

