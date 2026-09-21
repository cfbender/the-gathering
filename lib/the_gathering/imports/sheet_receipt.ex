defmodule TheGathering.Imports.SheetReceipt do
  @moduledoc false
  use Ecto.Schema

  @primary_key {:key, :string, autogenerate: false}
  schema "sheet_import_receipts" do
    belongs_to :game, TheGathering.Games.Game
  end
end
