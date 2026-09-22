defmodule TheGathering.Repo.Migrations.CreateDiscordResultDrafts do
  use Ecto.Migration

  def change do
    create table(:discord_result_drafts, primary_key: false) do
      add :id, :binary_id, primary_key: true

      add :pending_game_id, references(:pending_discord_games, on_delete: :delete_all),
        null: false

      add :discord_id, :string, null: false
      add :guild_id, :string, null: false
      add :channel_id, :string, null: false
      add :snapshot, :binary, null: false
      add :data, :map, null: false, default: %{}
      add :expires_at, :utc_datetime, null: false
    end

    create index(:discord_result_drafts, [:pending_game_id])
    create index(:discord_result_drafts, [:expires_at])
  end
end
