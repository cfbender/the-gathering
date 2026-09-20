defmodule TheGatheringWeb.API.MythicTrackImportJSON do
  def preview(%{preview: preview}),
    do: %{data: Map.update!(preview, :games, &Enum.map(&1, fn game -> mythic_game(game) end))}

  def result(%{result: result}), do: %{data: result}

  defp mythic_game(game) do
    game
    |> Map.from_struct()
    |> Map.update!(:seats, &Enum.map(&1, fn seat -> mythic_seat(seat) end))
  end

  defp mythic_seat(seat) do
    seat
    |> Map.from_struct()
    |> Map.put(:partner, seat.partner_name)
    |> Map.delete(:partner_name)
  end
end
