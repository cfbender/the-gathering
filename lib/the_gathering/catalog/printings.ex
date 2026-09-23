defmodule TheGathering.Catalog.Printings do
  @moduledoc false

  alias TheGathering.Catalog.{CardData, Printing, Scryfall}
  alias TheGathering.Repo

  def list(card, page) do
    with {:ok, cards, has_more} <- Scryfall.printings(card.oracle_id, page) do
      rows =
        cards
        |> Enum.filter(
          &(&1["oracle_id"] == card.oracle_id and "paper" in &1["games"] and &1["lang"] == "en")
        )
        |> Enum.map(&printing_data/1)

      Repo.insert_all(Printing, rows, on_conflict: :replace_all, conflict_target: :id)
      {:ok, Enum.map(rows, &struct!(Printing, &1)), has_more}
    end
  end

  @doc """
  Full details of one printing by Scryfall id: rules text, cost, type, set name and images.

  The catalog keeps one printing per card, so an arbitrary printing (a webcam-table
  recognition result, for example) is fetched from Scryfall. Its image and set are cached in
  `card_printings` on the way through, like the printing picker does.
  """
  def details(id) do
    with {:ok, card} <- Scryfall.card(id),
         %{} <- CardData.from_scryfall(card) do
      row = printing_data(card)
      Repo.insert_all(Printing, [row], on_conflict: :replace_all, conflict_target: :id)
      {:ok, details_data(card, row)}
    else
      # Tokens and memorabilia are not cards the catalog describes.
      nil -> {:error, :not_found}
      {:error, _reason} = error -> error
    end
  end

  defp details_data(card, row) do
    front = card |> Map.get("card_faces", []) |> List.first() || %{}
    faces = Map.get(card, "card_faces", [])

    Map.merge(row, %{
      mana_cost: Map.get(card, "mana_cost") || Map.get(front, "mana_cost"),
      type_line: Map.get(card, "type_line") || Map.get(front, "type_line") || "",
      oracle_text: face_text(card, faces, "oracle_text"),
      flavor_text: face_text(card, faces, "flavor_text"),
      power: Map.get(card, "power") || Map.get(front, "power"),
      toughness: Map.get(card, "toughness") || Map.get(front, "toughness"),
      loyalty: Map.get(card, "loyalty") || Map.get(front, "loyalty"),
      layout: Map.get(card, "layout", "normal"),
      rarity: Map.get(card, "rarity"),
      released_at: Map.get(card, "released_at"),
      scryfall_uri: Map.get(card, "scryfall_uri")
    })
  end

  # Multi-face cards keep their text per face; join it so the preview shows every half.
  defp face_text(card, faces, key) do
    case Map.get(card, key) do
      text when is_binary(text) ->
        text

      nil ->
        faces
        |> Enum.map(&Map.get(&1, key))
        |> Enum.reject(&is_nil/1)
        |> case do
          [] -> nil
          texts -> Enum.join(texts, "\n//\n")
        end
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
