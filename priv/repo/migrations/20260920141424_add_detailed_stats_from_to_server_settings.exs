defmodule TheGathering.Repo.Migrations.AddDetailedStatsFromToServerSettings do
  use Ecto.Migration

  def change do
    alter table(:server_settings) do
      # Games played before this date count toward win/loss records only; their
      # seat, duration, turn, and MVP data are left out of statistics.
      add :detailed_stats_from, :date
    end
  end
end
