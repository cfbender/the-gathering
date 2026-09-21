defmodule TheGathering.Repo.Migrations.AddWinConditionToGames do
  use Ecto.Migration

  def change do
    alter table(:games) do
      add :win_condition, :text
    end
  end
end
