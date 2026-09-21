defmodule TheGathering.Repo.Migrations.AddDeckPrintings do
  use Ecto.Migration

  def change do
    create table(:card_printings, primary_key: false) do
      add :id, :string, primary_key: true
      add :oracle_id, :string, null: false
      add :name, :string, null: false
      add :set_code, :string, null: false
      add :set_name, :string, null: false
      add :collector_number, :string, null: false
      add :lang, :string, null: false, default: "en"
      add :image_uris, :map, null: false
    end

    alter table(:decks) do
      add :commander_printing_id, references(:card_printings, type: :string)
      add :partner_printing_id, references(:card_printings, type: :string)
    end
  end
end
