defmodule TheGathering.Catalog.Printing do
  @moduledoc "A printing cached independently of the replaceable identity catalog."

  use Ecto.Schema

  @primary_key {:id, :string, autogenerate: false}
  schema "card_printings" do
    field :oracle_id, :string
    field :name, :string
    field :set_code, :string
    field :set_name, :string
    field :collector_number, :string
    field :lang, :string, default: "en"
    field :image_uris, :map
    field :game_changer, :boolean, default: false
  end
end
