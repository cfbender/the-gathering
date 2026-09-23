defmodule TheGathering.Repo.Migrations.CreateWebcamTableSessions do
  use Ecto.Migration

  def change do
    create table(:webcam_table_sessions, primary_key: false) do
      add :id, :string, primary_key: true
      add :snapshot, :binary, null: false
      add :expires_at, :utc_datetime_usec, null: false
    end

    create index(:webcam_table_sessions, [:expires_at])
  end
end
