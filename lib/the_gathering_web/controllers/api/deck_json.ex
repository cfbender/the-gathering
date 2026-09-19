defmodule TheGatheringWeb.API.DeckJSON do
  alias TheGathering.Games.Deck
  alias TheGatheringWeb.API.{GameJSON, PlayerJSON}

  def index(%{decks: decks}), do: %{data: Enum.map(decks, &summary/1)}
  def show(%{deck: deck}), do: %{data: detail(deck)}

  def summary(%Deck{} = deck) do
    %{
      id: deck.id,
      player_id: deck.player_id,
      name: deck.name,
      commander_card_id: deck.commander_card_id,
      commander_name: deck.commander_name,
      partner_card_id: deck.partner_card_id,
      partner_name: deck.partner_name,
      color_identity: deck.color_identity,
      decklist_url: deck.decklist_url,
      decklist_source: deck.decklist_source,
      archived_at: deck.archived_at,
      player: player(deck)
    }
  end

  defp detail(deck) do
    summary(deck)
    |> Map.put(:games_played, length(deck.game_players))
    |> Map.put(:wins, Enum.count(deck.game_players, &(&1.result == "win")))
    |> Map.put(
      :recent_games,
      deck.game_players |> Enum.take(10) |> Enum.map(&GameJSON.seat_game/1)
    )
  end

  defp player(%{player: %Ecto.Association.NotLoaded{}}), do: nil
  defp player(%{player: nil}), do: nil
  defp player(%{player: player}), do: PlayerJSON.summary(player)
end
