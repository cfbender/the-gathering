defmodule TheGatheringWeb.API.RemoteDeckJSON do
  def index(%{result: result}) do
    %{
      data: %{
        decks: Enum.map(result.decks, &deck/1),
        sources: Enum.map(result.sources, &source/1)
      }
    }
  end

  defp deck(deck) do
    %{
      name: deck.name,
      commanders: deck.commanders,
      color_identity: deck.color_identity,
      url: deck.url,
      source: deck.source,
      updated_at: deck.updated_at
    }
  end

  defp source(source) do
    %{source: source.source, configured: source.configured, error: source.error}
  end
end
