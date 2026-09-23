defmodule TheGathering.Catalog.StagedCard do
  @moduledoc false

  use Ecto.Schema

  @primary_key false
  schema "catalog_cards_staging" do
    field :id, :string, primary_key: true
    field :oracle_id, :string
    field :name, :string
    field :normalized_name, :string
    field :mana_cost, :string
    field :cmc, :float
    field :type_line, :string
    field :oracle_text, :string
    field :colors, {:array, :string}
    field :color_identity, {:array, :string}
    field :image_uris, :map
    field :set_code, :string
    field :collector_number, :string
    field :released_at, :date
    field :layout, :string
    field :rarity, :string
    field :game_changer, :boolean, default: false
    field :commander_legal, :boolean
    field :can_be_commander, :boolean
    field :commander_pairing, :string
    field :selection_key, :string

    timestamps(type: :utc_datetime)
  end
end
