defmodule TheGathering.Decklists.Sources.Moxfield do
  @moduledoc false

  @behaviour TheGathering.Decklists.Source

  alias TheGathering.Decklists.{Decklist, HTTP}

  @impl true
  def resolve(parsed) do
    url = "https://api2.moxfield.com/v3/decks/all/#{parsed.id}"

    case HTTP.get(url) do
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
    commanders = get_in(body, ["boards", "commanders", "cards"]) || %{}

    %Decklist{
      source: :moxfield,
      id: parsed.id,
      url: parsed.canonical_url,
      name: body["name"],
      commanders: commander_names(commanders),
      color_identity: commander_colors(commanders),
      author:
        get_in(body, ["createdByUser", "displayName"]) ||
          get_in(body, ["createdByUser", "userName"]),
      card_count: board_count(body, "mainboard") + board_count(body, "commanders"),
      fetched_at: DateTime.utc_now()
    }
  end

  defp commander_names(cards) do
    Enum.map(cards, fn {_id, entry} -> %{name: get_in(entry, ["card", "name"])} end)
  end

  defp commander_colors(cards) do
    colors =
      cards
      |> Enum.flat_map(fn {_id, entry} -> get_in(entry, ["card", "color_identity"]) || [] end)
      |> Enum.uniq()

    if colors == [], do: nil, else: order_colors(colors)
  end

  defp board_count(body, board), do: get_in(body, ["boards", board, "count"]) || 0
  defp order_colors(colors), do: Enum.filter(~w(W U B R G), &(&1 in colors))
end
