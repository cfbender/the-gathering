defmodule TheGathering.Repo.Migrations.CreateCardRulingsCache do
  use Ecto.Migration

  def change do
    create table(:card_rulings_cache, primary_key: false) do
      add :id, :string, primary_key: true
      add :rulings, {:array, :map}, null: false
      add :fetched_at, :utc_datetime, null: false
    end
  end
end
