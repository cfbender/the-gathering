defmodule TheGathering.Decklists.Sources.Manavault do
  @moduledoc false

  @behaviour TheGathering.Decklists.Source

  alias TheGathering.Decklists.{Decklist, HTTP}

  # The public share schema clamps `first` to 500; a Commander deck fits in one page, but
  # follow the cursor a few times so a large list is not silently cut short.
  @max_pages 4

  @query """
  query SharedDeck($id: ID!, $after: String) {
    deck(id: $id) {
      name
      cardCount
      commanderColorIdentity
      deckCards(first: 500, after: $after) {
        pageInfo { endCursor hasNextPage }
        edges {
          node {
            quantity
            zone
            card { name }
            preferredPrinting { scryfallId }
            fallbackPrinting { scryfallId }
          }
        }
      }
    }
  }
  """

  @impl true
  def resolve(parsed) do
    with {:ok, deck} <- fetch_page(parsed.id, nil),
         {:ok, edges} <- remaining_edges(parsed.id, deck, 1) do
      {:ok, from_response(deck, edges, parsed)}
    end
  end

  defp remaining_edges(id, deck, page) do
    edges = get_in(deck, ["deckCards", "edges"]) || []

    case get_in(deck, ["deckCards", "pageInfo"]) do
      %{"hasNextPage" => true, "endCursor" => cursor}
      when is_binary(cursor) and page < @max_pages ->
        with {:ok, next} <- fetch_page(id, cursor),
             {:ok, rest} <- remaining_edges(id, next, page + 1) do
          {:ok, edges ++ rest}
        end

      _ ->
        {:ok, edges}
    end
  end

  defp fetch_page(id, after_cursor) do
    body = %{query: @query, variables: %{id: id, after: after_cursor}}

    case HTTP.post("#{TheGathering.Decklists.manavault_url()}/share/graphql", body) do
      {:ok, %Req.Response{status: 200, body: %{"data" => %{"deck" => deck}}}}
      when is_map(deck) ->
        {:ok, deck}

      {:ok, %Req.Response{status: 200, body: %{"data" => %{"deck" => nil}}}} ->
        {:error, :not_found}

      {:ok, %Req.Response{status: 404}} ->
        {:error, :not_found}

      {:ok, %Req.Response{status: status}} when status in [401, 403] ->
        {:error, :private}

      _ ->
        {:error, :upstream_error}
    end
  end

  defp from_response(body, edges, parsed) do
    nodes = Enum.map(edges, & &1["node"])

    commanders =
      nodes
      |> Enum.filter(&(&1["zone"] == "commander"))
      |> Enum.map(&%{name: get_in(&1, ["card", "name"])})

    colors = body["commanderColorIdentity"]

    %Decklist{
      source: :manavault,
      id: parsed.id,
      url: parsed.canonical_url,
      name: body["name"],
      commanders: commanders,
      color_identity: if(colors == [], do: nil, else: colors),
      author: nil,
      card_count: body["cardCount"],
      cards: deck_cards(nodes),
      fetched_at: DateTime.utc_now()
    }
  end

  # `considering` (the merged maybe- and sideboard) is not part of the deck.
  defp deck_cards(nodes) do
    nodes
    |> Enum.filter(&(&1["zone"] in ["commander", "mainboard"]))
    |> Enum.map(fn node ->
      zone = if node["zone"] == "commander", do: :commander, else: :mainboard

      printing =
        get_in(node, ["preferredPrinting", "scryfallId"]) ||
          get_in(node, ["fallbackPrinting", "scryfallId"])

      Decklist.card(get_in(node, ["card", "name"]), node["quantity"], zone, printing)
    end)
    |> Enum.reject(&is_nil/1)
  end
end
