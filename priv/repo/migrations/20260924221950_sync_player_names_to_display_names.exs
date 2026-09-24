defmodule TheGathering.Repo.Migrations.SyncPlayerNamesToDisplayNames do
  use Ecto.Migration

  # Display name edits used to update only the user, leaving the linked player
  # (what games and stats show) on its original name, usually the Discord handle.
  # Copy each display name onto its player unless another player already holds it
  # or more than one linked account wants the same name.
  def up do
    execute("""
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
      )
    """)
  end

  def down, do: :ok
end
