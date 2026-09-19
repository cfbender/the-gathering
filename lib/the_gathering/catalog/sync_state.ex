defmodule TheGathering.Catalog.SyncState do
  @moduledoc "Persistent state for the most recent catalog synchronization."

  use Ecto.Schema

  schema "catalog_syncs" do
    field :status, :string, default: "never"
    field :last_started_at, :utc_datetime
    field :last_finished_at, :utc_datetime
    field :card_count, :integer, default: 0
    field :scryfall_updated_at, :utc_datetime
    field :last_error, :string

    timestamps(type: :utc_datetime)
  end
end
