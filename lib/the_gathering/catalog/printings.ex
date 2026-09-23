defmodule TheGathering.Catalog.Printings do
  @moduledoc false

  alias TheGathering.Catalog.{Printing, PrintingId, Scryfall}
  alias TheGathering.Repo

  @face_layouts ~w(transform modal_dfc reversible_card double_faced_token)
  @face_fields ~w(name image_uris mana_cost type_line oracle_text flavor_text power toughness loyalty)

  def list(card, page) do
    with {:ok, cards, has_more} <- Scryfall.printings(card.oracle_id, page) do
      rows =
        cards
        |> Enum.filter(
          &(&1["oracle_id"] == card.oracle_id and "paper" in &1["games"] and &1["lang"] == "en")
        )
        # Scryfall includes memorabilia basics; CardData intentionally does not describe them.
        |> Enum.reject(&(&1["set_type"] in ["token", "memorabilia"]))
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
    with {:ok, card_id, face} <- PrintingId.parse(id),
         {:ok, card} <- Scryfall.card(card_id),
         {:ok, card} <- select_face(card, face, id) do
      row = printing_data(card)
      Repo.insert_all(Printing, [row], on_conflict: :replace_all, conflict_target: :id)
      {:ok, details_data(card, row)}
    end
  end

  defp select_face(%{"layout" => layout, "card_faces" => faces} = card, index, id)
       when layout in @face_layouts do
    case Enum.at(faces, index) do
      %{"name" => _name} = face ->
        {:ok,
         card
         |> Map.drop(["card_faces" | @face_fields])
         |> Map.merge(Map.take(face, ["oracle_id" | @face_fields]))
         |> Map.put("id", id)}

      _ ->
        {:error, :bad_request}
    end
  end

  defp select_face(card, 0, _id), do: {:ok, card}
  defp select_face(_card, _index, _id), do: {:error, :bad_request}

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
      prices: Map.new(["usd", "usd_foil", "usd_etched"], &{&1, get_in(card, ["prices", &1])}),
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
    front = card |> Map.get("card_faces", []) |> List.first() || %{}
    images = card["image_uris"] || front["image_uris"] || %{}

    %{
      id: card["id"],
      oracle_id: card["oracle_id"],
      name: card["name"],
      set_code: card["set"],
      set_name: card["set_name"],
      collector_number: card["collector_number"],
      lang: card["lang"],
      image_uris: Map.take(images, ["small", "normal", "art_crop"])
    }
  end
end
