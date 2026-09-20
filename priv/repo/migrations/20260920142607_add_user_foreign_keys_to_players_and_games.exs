defmodule TheGathering.Repo.Migrations.AddUserForeignKeysToPlayersAndGames do
  use Ecto.Migration

  @disable_ddl_transaction true

  def up do
    with_foreign_keys_disabled(fn ->
      query("""
      UPDATE players
      SET user_id = NULL
      WHERE user_id IS NOT NULL
        AND NOT EXISTS (SELECT 1 FROM users WHERE users.id = players.user_id)
      """)

      rebuild_players(true)

      query("""
      UPDATE games
      SET created_by_user_id = NULL
      WHERE created_by_user_id IS NOT NULL
        AND NOT EXISTS (SELECT 1 FROM users WHERE users.id = games.created_by_user_id)
      """)

      rebuild_games(true)
    end)
  end

  def down do
    with_foreign_keys_disabled(fn ->
      rebuild_players(false)
      rebuild_games(false)
    end)
  end

  defp with_foreign_keys_disabled(callback) do
    repo().checkout(fn ->
      query("PRAGMA foreign_keys = OFF")
      query("PRAGMA legacy_alter_table = ON")

      try do
        callback.()
      after
        query("PRAGMA legacy_alter_table = OFF")
        query("PRAGMA foreign_keys = ON")
      end
    end)
  end

  defp rebuild_players(with_foreign_key?) do
    drop_player_indexes()
    query("ALTER TABLE players RENAME TO players_old")

    foreign_key =
      if with_foreign_key?,
        do:
          ", CONSTRAINT players_user_id_fkey" <>
            " FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE RESTRICT",
        else: ""

    query("""
    CREATE TABLE players (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL,
      user_id INTEGER,
      discord_id TEXT,
      archived_at TEXT,
      inserted_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
      #{foreign_key}
    )
    """)

    query("""
    INSERT INTO players
      (id, name, user_id, discord_id, archived_at, inserted_at, updated_at)
    SELECT id, name, user_id, discord_id, archived_at, inserted_at, updated_at
    FROM players_old
    """)

    query("DROP TABLE players_old")
    create_player_indexes()
  end

  defp rebuild_games(with_foreign_key?) do
    drop_game_indexes()
    query("ALTER TABLE games RENAME TO games_old")

    foreign_key =
      if with_foreign_key?,
        do:
          ", CONSTRAINT games_created_by_user_id_fkey" <>
            " FOREIGN KEY (created_by_user_id) REFERENCES users(id) ON DELETE RESTRICT",
        else: ""

    query("""
    CREATE TABLE games (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      played_at TEXT NOT NULL,
      duration_minutes INTEGER,
      turns INTEGER,
      notes TEXT,
      source TEXT NOT NULL DEFAULT 'manual',
      external_id TEXT,
      created_by_user_id INTEGER,
      inserted_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
      #{foreign_key}
    )
    """)

    query("""
    INSERT INTO games
      (id, played_at, duration_minutes, turns, notes, source, external_id,
       created_by_user_id, inserted_at, updated_at)
    SELECT id, played_at, duration_minutes, turns, notes, source, external_id,
           created_by_user_id, inserted_at, updated_at
    FROM games_old
    """)

    query("DROP TABLE games_old")
    create_game_indexes()
  end

  defp create_player_indexes do
    query("CREATE UNIQUE INDEX players_user_id_index ON players (user_id)")
    query("CREATE UNIQUE INDEX players_discord_id_index ON players (discord_id)")
    query("CREATE UNIQUE INDEX players_name_nocase_index ON players (name COLLATE NOCASE)")
  end

  defp drop_player_indexes do
    query("DROP INDEX players_user_id_index")
    query("DROP INDEX players_discord_id_index")
    query("DROP INDEX players_name_nocase_index")
  end

  defp create_game_indexes do
    query("""
    CREATE UNIQUE INDEX games_source_external_id_index
    ON games (source, external_id)
    WHERE external_id IS NOT NULL
    """)

    query("CREATE INDEX games_played_at_index ON games (played_at)")
  end

  defp drop_game_indexes do
    query("DROP INDEX games_source_external_id_index")
    query("DROP INDEX games_played_at_index")
  end

  defp query(sql), do: Ecto.Adapters.SQL.query!(repo(), sql, [])
end
