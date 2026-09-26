defmodule TheGatheringWeb.API.DecklistJSON do
  alias TheGathering.Catalog.{Card, CardImages}
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

  def cards(%{decklist: %Decklist{} = decklist, catalog: catalog}) do
    %{
      data: %{
        source: decklist.source,
        url: decklist.url,
        name: decklist.name,
        fetched_at: DateTime.to_iso8601(decklist.fetched_at),
        cards: Enum.map(decklist.cards, &card(&1, Map.get(catalog, &1.name)))
      }
    }
  end

  defp card(entry, catalog_card) do
    {details, catalog_images} = catalog_details(catalog_card)

    Map.merge(details, %{
      name: entry.name,
      quantity: entry.quantity,
      zone: entry.zone,
      printing_id: entry.printing_id,
      # The list's own printing, so the dialog shows the player's art; the catalog's
      # preferred printing otherwise.
      image_uris: CardImages.printing_urls(entry.printing_id) || catalog_images
    })
  end

  defp catalog_details(%Card{} = card) do
    {%{
       card_id: card.id,
       type_line: card.type_line,
       mana_cost: card.mana_cost,
       cmc: card.cmc,
       game_changer: card.game_changer
     }, CardImages.urls(Map.take(card.image_uris || %{}, ~w(small normal)))}
  end

  defp catalog_details(nil) do
    {%{card_id: nil, type_line: nil, mana_cost: nil, cmc: nil, game_changer: false}, %{}}
  end
end
