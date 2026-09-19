defmodule TheGathering.Repo.Migrations.CreateUsersAndServerSettings do
  use Ecto.Migration

  def change do
    create table(:users) do
      add :username, :string, null: false
      add :display_name, :string, null: false
      add :hashed_password, :string, null: false
      add :role, :string, null: false, default: "member"
      add :disabled_at, :utc_datetime

      timestamps(type: :utc_datetime)
    end

    create unique_index(:users, [:username])

    create table(:server_settings, primary_key: false) do
      add :id, :integer, primary_key: true
      add :registration_enabled, :boolean, null: false, default: false

      timestamps(type: :utc_datetime)
    end

    execute(
      "INSERT INTO server_settings (id, registration_enabled, inserted_at, updated_at) VALUES (1, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
      "DELETE FROM server_settings WHERE id = 1"
    )
  end
end
