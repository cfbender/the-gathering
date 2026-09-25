defmodule TheGathering.Catalog.DetailsCache do
  @moduledoc "Printing details cached by printing id (face suffix included), independently of catalog refreshes."
  use Ecto.Schema

  @primary_key {:id, :string, autogenerate: false}
  schema "card_details_cache" do
    field :details, :map
    field :fetched_at, :utc_datetime
  end
end
