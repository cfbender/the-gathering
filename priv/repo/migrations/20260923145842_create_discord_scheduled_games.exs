defmodule TheGathering.Repo.Migrations.CreateDiscordScheduledGames do
  use Ecto.Migration

  def change do
    create table(:discord_scheduled_games) do
      add :guild_id, :string, null: false
      add :channel_id, :string, null: false
      add :message_id, :string
      add :host_discord_id, :string, null: false
      add :title, :string, null: false
      add :format, :string
      add :start_at, :utc_datetime
      add :min_players, :integer, null: false, default: 3
      add :status, :string, null: false, default: "open"
      add :room_id, :uuid
      add :players, :map, null: false, default: %{}
      add :announcement_id, :string
      add :message_dirty, :boolean, null: false, default: true
      timestamps(type: :utc_datetime)
    end

    create index(:discord_scheduled_games, [:status, :start_at])
    create index(:discord_scheduled_games, [:message_dirty])
  end
end
