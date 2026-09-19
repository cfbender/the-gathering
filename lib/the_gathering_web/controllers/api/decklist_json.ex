defmodule TheGatheringWeb.API.DecklistJSON do
  alias TheGathering.Decklists.Decklist

  def show(%{decklist: %Decklist{} = decklist}) do
    data = %{
      source: decklist.source,
      id: decklist.id,
      url: decklist.url,
      name: decklist.name,
      commanders: decklist.commanders,
      color_identity: decklist.color_identity,
      author: decklist.author,
      card_count: decklist.card_count,
      fetched_at: DateTime.to_iso8601(decklist.fetched_at)
    }

    %{data: Map.reject(data, fn {_key, value} -> is_nil(value) end)}
  end
end
