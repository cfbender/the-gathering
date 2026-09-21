defmodule TheGathering.Catalog.Printings do
  @moduledoc false

  alias TheGathering.Catalog.{CardData, Printing, Scryfall}
  alias TheGathering.Repo

  def list(card, page) do
    with {:ok, cards, has_more} <- Scryfall.printings(card.oracle_id, page) do
      rows =
        cards
        |> Enum.filter(&(&1["oracle_id"] == card.oracle_id and "paper" in &1["games"]))
        |> Enum.map(&printing_data/1)

      Repo.insert_all(Printing, rows, on_conflict: :replace_all, conflict_target: :id)
      {:ok, Enum.map(rows, &struct!(Printing, &1)), has_more}
    end
  end

  defp printing_data(card) do
    data = CardData.from_scryfall(card)

    data
    |> Map.take([:id, :oracle_id, :name, :set_code, :collector_number, :image_uris])
    |> Map.put(:set_name, card["set_name"])
    |> Map.put(:lang, card["lang"])
  end
end
