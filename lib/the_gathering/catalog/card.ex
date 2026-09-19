defmodule TheGathering.Catalog.Card do
  @moduledoc "A locally cached Scryfall card, represented by its preferred latest printing."

  use Ecto.Schema

  @primary_key false
  schema "cards" do
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
    field :commander_legal, :boolean
    field :can_be_commander, :boolean
    field :commander_pairing, :string

    timestamps(type: :utc_datetime)
  end
end
