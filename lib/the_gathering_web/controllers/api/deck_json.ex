defmodule TheGatheringWeb.API.DeckJSON do
  alias TheGathering.Catalog
  alias TheGathering.Games.Deck
  alias TheGatheringWeb.API.{GameJSON, PlayerJSON}

  def index(%{decks: decks, card_art: card_art}),
    do: %{data: Enum.map(decks, &summary(&1, card_art))}

  def show(%{deck: deck, card_art: card_art}), do: %{data: detail(deck, card_art)}

  def summary(%Deck{} = deck, card_art \\ %{}) do
    %{
      id: deck.id,
      player_id: deck.player_id,
      name: deck.name,
      commander_card_id: deck.commander_card_id,
      commander_name: deck.commander_name,
      commander_printing_id: deck.commander_printing_id,
      commander_art_crop_url:
        Catalog.art_crop_url(
          card_art,
          deck.commander_card_id,
          deck.commander_name,
          deck.commander_printing_id
        ),
      partner_card_id: deck.partner_card_id,
      partner_name: deck.partner_name,
      partner_printing_id: deck.partner_printing_id,
      partner_art_crop_url:
        Catalog.art_crop_url(
          card_art,
          deck.partner_card_id,
          deck.partner_name,
          deck.partner_printing_id
        ),
      color_identity: deck.color_identity,
      decklist_url: deck.decklist_url,
      decklist_source: deck.decklist_source,
      archived_at: deck.archived_at,
      skip_count: deck.skip_count,
      included_for_play: deck.included_for_play,
      player: player(deck)
    }
  end

  def card_refs(decks) when is_list(decks) do
    Enum.flat_map(decks, fn deck ->
      [
        {deck.commander_card_id, deck.commander_name},
        {deck.partner_card_id, deck.partner_name},
        {:printing, deck.commander_printing_id},
        {:printing, deck.partner_printing_id}
      ]
    end)
  end

  def card_refs(%Deck{} = deck), do: card_refs([deck])

  defp detail(deck, card_art) do
    summary(deck, card_art)
    |> Map.put(:games_played, length(deck.game_players))
    |> Map.put(:wins, Enum.count(deck.game_players, &(&1.result == "win")))
    |> Map.put(
      :recent_games,
      deck.game_players |> Enum.take(10) |> Enum.map(&GameJSON.seat_game(&1, card_art))
    )
  end

  defp player(%{player: %Ecto.Association.NotLoaded{}}), do: nil
  defp player(%{player: nil}), do: nil
  defp player(%{player: player}), do: PlayerJSON.summary(player)
end
