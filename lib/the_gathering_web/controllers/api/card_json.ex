defmodule TheGatheringWeb.API.CardJSON do
  alias TheGathering.Catalog.Card

  def index(%{cards: cards}), do: %{data: Enum.map(cards, &summary/1)}
  def show(%{card: card}), do: %{data: detail(card)}

  defp summary(%Card{} = card) do
    %{
      id: card.id,
      oracle_id: card.oracle_id,
      name: card.name,
      mana_cost: card.mana_cost,
      type_line: card.type_line,
      color_identity: card.color_identity,
      image_uris: Map.take(card.image_uris || %{}, ["small", "normal", "art_crop"]),
      can_be_commander: card.can_be_commander,
      commander_pairing: card.commander_pairing
    }
  end

  defp detail(%Card{} = card) do
    summary(card)
    |> Map.merge(%{
      cmc: card.cmc,
      oracle_text: card.oracle_text,
      colors: card.colors,
      set_code: card.set_code,
      collector_number: card.collector_number,
      released_at: card.released_at,
      layout: card.layout,
      rarity: card.rarity,
      commander_legal: card.commander_legal
    })
  end
end
