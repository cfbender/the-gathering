defmodule TheGathering.Repo.Migrations.CreateApiKeys do
  use Ecto.Migration

  def change do
    create table(:api_keys) do
      add :user_id, references(:users, on_delete: :delete_all), null: false
      add :name, :string, null: false
      add :token_hash, :binary, null: false
      add :prefix, :string, null: false
      add :last_used_at, :utc_datetime

      timestamps(type: :utc_datetime, updated_at: false)
    end

    create index(:api_keys, [:user_id])
    create unique_index(:api_keys, [:token_hash])
  end
end
