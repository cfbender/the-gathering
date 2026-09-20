defmodule TheGathering.Repo.Migrations.CreatePendingDiscordGames do
  use Ecto.Migration

  def change do
    create table(:pending_discord_games) do
      add :external_id, :string, null: false
      add :guild_id, :string, null: false
      add :channel_id, :string, null: false
      add :played_at, :utc_datetime, null: false
      add :players, :map, null: false
      add :raw, :map, null: false, default: %{}

      timestamps(type: :utc_datetime)
    end

    create unique_index(:pending_discord_games, [:external_id])
    create index(:pending_discord_games, [:channel_id, :played_at])
  end
end
