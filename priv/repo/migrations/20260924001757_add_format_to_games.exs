defmodule TheGathering.Repo.Migrations.AddFormatToGames do
  use Ecto.Migration

  def change do
    alter table(:games) do
      add :format, :string, null: false, default: "commander"
    end
  end
end
