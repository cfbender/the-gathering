defmodule TheGathering.Repo.Migrations.AddSheetImportReceipts do
  use Ecto.Migration

  def change do
    create table(:sheet_import_receipts, primary_key: false) do
      add :key, :string, primary_key: true
      add :game_id, references(:games, on_delete: :delete_all), null: false
    end

    create index(:sheet_import_receipts, [:game_id])
  end
end
