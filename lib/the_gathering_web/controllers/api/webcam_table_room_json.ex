defmodule TheGatheringWeb.API.WebcamTableRoomJSON do
  def index(%{rooms: rooms}) do
    %{data: Enum.map(rooms, &room/1)}
  end

  defp room(room) do
    %{
      id: room.id,
      started_at: room.started_at,
      full: room.full,
      players: room.players
    }
  end
end
