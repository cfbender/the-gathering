defmodule TheGathering.Repo.Migrations.IncludeCommanderColorsInDeckIdentities do
  use Ecto.Migration

  import Ecto.Query

  alias TheGathering.Catalog.CardData

  @order ~w(W U B R G)

  # Some deck writers stored only the first commander's colors for partner decks.
  # Decks now always include every commander card's identity, so widen the stored
  # identities once; colors already recorded (such as a chosen color) are kept.
  def up do
    decks =
      from(deck in "decks",
        select:
          {deck.id, deck.color_identity, deck.commander_card_id, deck.commander_name,
           deck.partner_card_id, deck.partner_name}
      )
      |> repo().all()

    ids = decks |> Enum.flat_map(&[elem(&1, 2), elem(&1, 4)]) |> Enum.reject(&is_nil/1)

    names =
      decks
      |> Enum.flat_map(&[elem(&1, 3), elem(&1, 5)])
      |> Enum.filter(&is_binary/1)
      |> Enum.map(&CardData.normalize_name/1)

    cards =
      from(card in "cards",
        where: card.id in ^Enum.uniq(ids) or card.normalized_name in ^Enum.uniq(names),
        select: {card.id, card.normalized_name, card.color_identity}
      )
      |> repo().all()

    by_id = Map.new(cards, fn {id, _name, colors} -> {id, decode(colors)} end)
    by_name = Map.new(cards, fn {_id, name, colors} -> {name, decode(colors)} end)

    Enum.each(decks, fn {id, identity, commander_id, commander_name, partner_id, partner_name} ->
      colors =
        [{commander_id, commander_name}, {partner_id, partner_name}]
        |> Enum.flat_map(&card_colors(&1, by_id, by_name))

      letters = String.graphemes(identity || "") ++ colors
      widened = @order |> Enum.filter(&(&1 in letters)) |> Enum.join()

      if widened != (identity || "") do
        repo().update_all(from(deck in "decks", where: deck.id == ^id),
          set: [color_identity: widened]
        )
      end
    end)
  end

  def down, do: :ok

  defp card_colors({id, name}, by_id, by_name) do
    Map.get(by_id, id) ||
      (is_binary(name) && Map.get(by_name, CardData.normalize_name(name))) || []
  end

  defp decode(colors) when is_list(colors), do: colors
  defp decode(colors) when is_binary(colors), do: Jason.decode!(colors)
  defp decode(_colors), do: []
end
