defmodule TheGathering.Repo.Migrations.CreateCardDetailsCache do
  use Ecto.Migration

  def change do
    create table(:card_details_cache, primary_key: false) do
      add :id, :string, primary_key: true
      add :details, :map, null: false
      add :fetched_at, :utc_datetime, null: false
    end
  end
end
