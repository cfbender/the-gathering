defmodule TheGatheringWeb.WebcamTableRooms do
  @moduledoc """
  Which webcam tables are open right now.

  Every running room is listed, including empty ones, until
  `TheGathering.WebcamTables.Pruner` closes it after 30 idle minutes. Presence
  supplies each room's connected seats, not admission or durable game state.
  Spectators do not appear as seats in the lobby.
  """

  alias TheGathering.WebcamTables
  alias TheGatheringWeb.Presence

  @lobby_topic "webcam_tables"
  @max_players 10

  @doc "Registers the calling channel process's seat in the lobby."
  def track_seat(room_id, participant) do
    Presence.track(self(), @lobby_topic, participant.peer_id, %{
      room_id: room_id,
      player_id: participant.player_id,
      player_name: participant.player_name,
      joined_at: participant.joined_at
    })
  end

  @doc "Open rooms, oldest first, each with its connected seated players in join order."
  def active_rooms do
    seats =
      @lobby_topic
      |> Presence.list()
      |> Enum.flat_map(fn {_peer_id, %{metas: metas}} -> metas end)
      |> Enum.group_by(& &1.room_id)

    WebcamTables.rooms()
    |> Enum.map(fn room ->
      seats =
        seats
        |> Map.get(room.id, [])
        |> Enum.uniq_by(& &1.player_id)
        |> Enum.sort_by(& &1.joined_at)

      %{
        id: room.id,
        started_at: room.opened_at,
        full: length(seats) >= @max_players,
        players: Enum.map(seats, &%{id: &1.player_id, name: &1.player_name})
      }
    end)
    |> Enum.sort_by(& &1.started_at)
  end
end
