defmodule TheGathering.Decklists.Sources.Archidekt do
  @moduledoc false

  @behaviour TheGathering.Decklists.Source

  alias TheGathering.Decklists.{Decklist, HTTP}

  @color_codes %{"White" => "W", "Blue" => "U", "Black" => "B", "Red" => "R", "Green" => "G"}

  @impl true
  def resolve(parsed) do
    case HTTP.get("https://archidekt.com/api/decks/#{parsed.id}/") do
      {:ok, %Req.Response{status: 200, body: body}} when is_map(body) ->
        {:ok, from_response(body, parsed)}

      {:ok, %Req.Response{status: 404}} ->
        {:error, :not_found}

      {:ok, %Req.Response{status: status}} when status in [401, 403] ->
        {:error, :private}

      _ ->
        {:error, :upstream_error}
    end
  end

  defp from_response(body, parsed) do
    excluded = excluded_categories(body)
    cards = Enum.reject(body["cards"] || [], &(List.first(&1["categories"] || []) in excluded))
    commanders = Enum.filter(cards, &("Commander" in (&1["categories"] || [])))

    %Decklist{
      source: :archidekt,
      id: parsed.id,
      url: parsed.canonical_url,
      name: body["name"],
      commanders: Enum.map(commanders, &%{name: card_name(&1)}),
      color_identity: commander_colors(commanders),
      author: get_in(body, ["owner", "username"]),
      card_count: Enum.sum(Enum.map(cards, &(&1["quantity"] || 0))),
      cards: deck_cards(cards),
      fetched_at: DateTime.utc_now()
    }
  end

  defp card_name(entry), do: get_in(entry, ["card", "oracleCard", "name"])

  # Archidekt boards are categories: Maybeboard, Sideboard and custom ones can be marked as
  # not part of the deck, and a card lives where its first (primary) category says.
  defp excluded_categories(body) do
    for %{"name" => name, "includedInDeck" => false} <- body["categories"] || [],
        into: MapSet.new(),
        do: name
  end

  defp deck_cards(cards) do
    cards
    |> Enum.map(fn entry ->
      zone = if "Commander" in (entry["categories"] || []), do: :commander, else: :mainboard
      Decklist.card(card_name(entry), entry["quantity"], zone, get_in(entry, ["card", "uid"]))
    end)
    |> Enum.reject(&is_nil/1)
  end

  defp commander_colors(commanders) do
    colors =
      commanders
      |> Enum.flat_map(&(get_in(&1, ["card", "oracleCard", "colorIdentity"]) || []))
      |> Enum.map(&Map.get(@color_codes, &1, &1))
      |> Enum.uniq()

    if colors == [], do: nil, else: Enum.filter(~w(W U B R G), &(&1 in colors))
  end
end
