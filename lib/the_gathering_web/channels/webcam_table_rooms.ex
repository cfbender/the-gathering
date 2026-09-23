defmodule TheGatheringWeb.WebcamTableRooms do
  @moduledoc """
  Which webcam tables are live right now.

  Rooms are unguessable ids minted in the browser, so nothing is stored about them. Every
  seated channel process also tracks itself on one lobby presence topic; listing that topic
  and grouping by room is the whole registry, and a seat disappears the moment its channel
  process does.
  """

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

  @doc "Live rooms, oldest first, each with its seated players in join order."
  def active_rooms do
    @lobby_topic
    |> Presence.list()
    |> Enum.flat_map(fn {_peer_id, %{metas: metas}} -> metas end)
    |> Enum.group_by(& &1.room_id)
    |> Enum.map(fn {room_id, seats} ->
      seats = seats |> Enum.uniq_by(& &1.player_id) |> Enum.sort_by(& &1.joined_at)

      %{
        id: room_id,
        started_at: seats |> Enum.map(& &1.joined_at) |> Enum.min(),
        full: length(seats) >= @max_players,
        players: Enum.map(seats, &%{id: &1.player_id, name: &1.player_name})
      }
    end)
    |> Enum.sort_by(& &1.started_at)
  end
end
