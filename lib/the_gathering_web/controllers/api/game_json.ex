defmodule TheGatheringWeb.API.GameJSON do
  alias TheGathering.Games.{Game, GamePlayer}
  alias TheGatheringWeb.API.{DeckJSON, PlayerJSON}

  def index(%{games: games, pagination: pagination}) do
    %{data: Enum.map(games, &game/1), pagination: pagination}
  end

  def show(%{game: game}), do: %{data: game(game)}

  def game(%Game{} = game) do
    %{
      id: game.id,
      played_at: game.played_at,
      duration_minutes: game.duration_minutes,
      turns: game.turns,
      notes: game.notes,
      source: game.source,
      external_id: game.external_id,
      created_by_user_id: game.created_by_user_id,
      seats: game.seats |> Enum.sort_by(& &1.seat) |> Enum.map(&seat/1)
    }
  end

  def seat(%GamePlayer{} = seat) do
    %{
      id: seat.id,
      player_id: seat.player_id,
      deck_id: seat.deck_id,
      seat: seat.seat,
      result: seat.result,
      eliminated_turn: seat.eliminated_turn,
      eliminated_by_player_id: seat.eliminated_by_player_id,
      mvp_card_id: seat.mvp_card_id,
      mvp_card_name: seat.mvp_card_name,
      notes: seat.notes,
      player: association(seat.player, &PlayerJSON.summary/1),
      deck: association(seat.deck, &DeckJSON.summary/1)
    }
  end

  def seat_game(%GamePlayer{} = seat) do
    %{
      id: seat.game.id,
      played_at: seat.game.played_at,
      result: seat.result,
      deck: association(seat.deck, &DeckJSON.summary/1)
    }
  end

  defp association(%Ecto.Association.NotLoaded{}, _mapper), do: nil
  defp association(nil, _mapper), do: nil
  defp association(value, mapper), do: mapper.(value)
end
