defmodule TheGathering.Repo.Migrations.AddMaybeToDiscordScheduledGames do
  use Ecto.Migration

  def change do
    alter table(:discord_scheduled_games) do
      add :maybe, :map, null: false, default: %{}
      add :maybe_pinged_at, :utc_datetime
      add :maybe_ping_id, :string
    end

    # The scheduler re-renders dirty messages, adding the Maybe button to open games.
    execute(
      "UPDATE discord_scheduled_games SET message_dirty = 1 WHERE status = 'open' AND message_id IS NOT NULL",
      fn -> :ok end
    )
  end
end
