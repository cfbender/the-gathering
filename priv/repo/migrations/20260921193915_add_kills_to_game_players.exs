defmodule TheGathering.Repo.Migrations.AddKillsToGamePlayers do
  use Ecto.Migration

  def change do
    alter table(:game_players) do
      add :kills, :integer
    end
  end
end
