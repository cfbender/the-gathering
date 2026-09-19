defmodule TheGathering.Repo.Migrations.CreateCatalogTables do
  use Ecto.Migration

  def change do
    create table(:cards, primary_key: false) do
      add :id, :string, primary_key: true
      add :oracle_id, :string, null: false
      add :name, :string, null: false
      add :normalized_name, :string, null: false
      add :mana_cost, :string
      add :cmc, :float, null: false, default: 0.0
      add :type_line, :string, null: false
      add :oracle_text, :text
      add :colors, :map, null: false, default: []
      add :color_identity, :map, null: false, default: []
      add :image_uris, :map, null: false, default: %{}
      add :set_code, :string, null: false
      add :collector_number, :string, null: false
      add :released_at, :date
      add :layout, :string, null: false
      add :rarity, :string, null: false
      add :commander_legal, :boolean, null: false, default: false
      add :can_be_commander, :boolean, null: false, default: false
      add :commander_pairing, :string

      timestamps(type: :utc_datetime)
    end

    create unique_index(:cards, [:oracle_id])
    create index(:cards, [:normalized_name])
    create index(:cards, [:can_be_commander, :normalized_name])

    # A complete generation is assembled here before one atomic swap.
    create table(:catalog_cards_staging, primary_key: false) do
      add :id, :string, primary_key: true
      add :oracle_id, :string, null: false
      add :name, :string, null: false
      add :normalized_name, :string, null: false
      add :mana_cost, :string
      add :cmc, :float, null: false, default: 0.0
      add :type_line, :string, null: false
      add :oracle_text, :text
      add :colors, :map, null: false, default: []
      add :color_identity, :map, null: false, default: []
      add :image_uris, :map, null: false, default: %{}
      add :set_code, :string, null: false
      add :collector_number, :string, null: false
      add :released_at, :date
      add :layout, :string, null: false
      add :rarity, :string, null: false
      add :commander_legal, :boolean, null: false, default: false
      add :can_be_commander, :boolean, null: false, default: false
      add :commander_pairing, :string
      add :selection_key, :string, null: false

      timestamps(type: :utc_datetime)
    end

    create unique_index(:catalog_cards_staging, [:oracle_id])

    create table(:catalog_syncs) do
      add :status, :string, null: false, default: "never"
      add :last_started_at, :utc_datetime
      add :last_finished_at, :utc_datetime
      add :card_count, :integer, null: false, default: 0
      add :scryfall_updated_at, :utc_datetime
      add :last_error, :text

      timestamps(type: :utc_datetime)
    end
  end
end
