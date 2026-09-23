defmodule TheGathering.Catalog.RulingCache do
  @moduledoc "Rulings cached by Scryfall printing id, independently of catalog refreshes."
  use Ecto.Schema

  @primary_key {:id, :string, autogenerate: false}
  schema "card_rulings_cache" do
    field :rulings, {:array, :map}
    field :fetched_at, :utc_datetime
  end
end
