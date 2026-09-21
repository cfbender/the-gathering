defmodule TheGatheringWeb.API.CSVImportJSON do
  def preview(%{preview: preview}),
    do: %{data: Map.update!(preview, :games, &Enum.map(&1, fn game -> csv_game(game) end))}

  def result(%{result: result}), do: %{data: result}

  defp csv_game(game) do
    game
    |> Map.from_struct()
    |> Map.update!(:seats, fn seats -> Enum.map(seats, &csv_seat(&1, game)) end)
  end

  defp csv_seat(seat, game) do
    %{
      game_id: game.game_id,
      date: game.played_at,
      player: seat.player,
      deck: seat.deck,
      commander: seat.commander,
      partner: seat.partner_name,
      seat: seat.seat,
      result: seat.result,
      kills: seat.kills,
      mvp_card: seat.mvp_card,
      duration_minutes: game.duration_minutes,
      turns: game.turns,
      notes: game.notes,
      line: seat.line
    }
  end
end
