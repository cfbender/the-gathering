defmodule TheGatheringWeb.API.GameJSON do
  alias TheGathering.Catalog
  alias TheGathering.Games.{Game, GamePlayer}
  alias TheGatheringWeb.API.{DeckJSON, PlayerJSON}

  def index(%{games: games, pagination: pagination, card_art: card_art}) do
    %{data: Enum.map(games, &game(&1, card_art)), pagination: pagination}
  end

  def show(%{game: game, card_art: card_art}), do: %{data: game(game, card_art)}

  def game(%Game{} = game, card_art \\ %{}) do
    %{
      id: game.id,
      played_at: game.played_at,
      duration_minutes: game.duration_minutes,
      turns: game.turns,
      win_condition: game.win_condition,
      notes: game.notes,
      source: game.source,
      format: game.format,
      external_id: game.external_id,
      created_by_user_id: game.created_by_user_id,
      seats: game.seats |> Enum.sort_by(& &1.seat) |> Enum.map(&seat(&1, card_art))
    }
  end

  def seat(%GamePlayer{} = seat, card_art \\ %{}) do
    %{
      id: seat.id,
      player_id: seat.player_id,
      deck_id: seat.deck_id,
      seat: seat.seat,
      result: seat.result,
      kills: seat.kills,
      eliminated_turn: seat.eliminated_turn,
      eliminated_by_player_id: seat.eliminated_by_player_id,
      mvp_card_id: seat.mvp_card_id,
      mvp_card_name: seat.mvp_card_name,
      mvp_game_changer: Catalog.game_changer?(card_art, seat.mvp_card_id, seat.mvp_card_name),
      mvp_image_url: Catalog.card_image_url(card_art, seat.mvp_card_id, seat.mvp_card_name),
      mvp_art_crop_url: Catalog.art_crop_url(card_art, seat.mvp_card_id, seat.mvp_card_name),
      notes: seat.notes,
      player: association(seat.player, &PlayerJSON.summary/1),
      deck: association(seat.deck, &DeckJSON.summary(&1, card_art))
    }
  end

  def seat_game(%GamePlayer{} = seat, card_art \\ %{}) do
    %{
      id: seat.game.id,
      played_at: seat.game.played_at,
      format: seat.game.format,
      result: seat.result,
      deck: association(seat.deck, &DeckJSON.summary(&1, card_art))
    }
  end

  def card_refs(games) when is_list(games), do: Enum.flat_map(games, &card_refs/1)

  def card_refs(%Game{} = game) do
    Enum.flat_map(game.seats, fn seat ->
      deck_refs = if seat.deck, do: DeckJSON.card_refs(seat.deck), else: []
      [{seat.mvp_card_id, seat.mvp_card_name} | deck_refs]
    end)
  end

  defp association(%Ecto.Association.NotLoaded{}, _mapper), do: nil
  defp association(nil, _mapper), do: nil
  defp association(value, mapper), do: mapper.(value)
end
