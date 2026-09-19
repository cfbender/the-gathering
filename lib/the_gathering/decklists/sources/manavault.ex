defmodule TheGathering.Decklists.Sources.Manavault do
  @moduledoc false

  @behaviour TheGathering.Decklists.Source

  alias TheGathering.Decklists.{Decklist, HTTP}

  @query """
  query SharedDeck($id: ID!) {
    deck(id: $id) {
      name
      cardCount
      commanderColorIdentity
      deckCards(first: 500) {
        edges { node { zone card { name } } }
      }
    }
  }
  """

  @impl true
  def resolve(parsed) do
    body = %{query: @query, variables: %{id: parsed.id}}

    case HTTP.post("#{TheGathering.Decklists.manavault_url()}/share/graphql", body) do
      {:ok, %Req.Response{status: 200, body: %{"data" => %{"deck" => deck}}}}
      when is_map(deck) ->
        {:ok, from_response(deck, parsed)}

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

  defp from_response(body, parsed) do
    edges = get_in(body, ["deckCards", "edges"]) || []

    commanders =
      edges
      |> Enum.filter(&(get_in(&1, ["node", "zone"]) == "commander"))
      |> Enum.map(&%{name: get_in(&1, ["node", "card", "name"])})

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
      fetched_at: DateTime.utc_now()
    }
  end
end
