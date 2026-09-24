defmodule TheGathering.WebcamTables do
  @moduledoc """
  Webcam tables: live rooms where remote players share cameras, life totals,
  turns, a shared clock and identified cards.

  Each room runs as its own `TheGathering.WebcamTables.Room` process, started
  on first join under `TheGathering.WebcamTables.RoomSupervisor` and
  registered by room id in `TheGathering.WebcamTables.Registry`. Rooms persist
  every change to `TheGathering.WebcamTables.Session` and stop when their last
  connection exits; `TheGathering.WebcamTables.Pruner` deletes expired sessions.

  Mutations must be called from the joined connection process: the room
  monitors it, identifies the seat's current connection by it, and messages
  it (`:seat_replaced`, `{:seat_eliminated, boolean}`).
  """

  alias TheGathering.WebcamTables.Room

  @doc """
  Admits `participant` with the calling process as its connection.

  Returns the table snapshot, the admitted participant (a returning player gets
  their saved seat back; late arrivals become spectators) and a monitor on the
  room held by the caller, which receives `:DOWN` if the room crashes.
  """
  def join(room, participant), do: join(room, participant, 3)

  defp join(room, participant, attempts) do
    pid = start_room(room)
    ref = Process.monitor(pid)

    try do
      GenServer.call(pid, {:join, participant})
    catch
      # The room stopped as its last connection left, just before this join
      # reached it. The registry forgets it once it has exited, so start anew.
      :exit, {reason, _call} when reason in [:normal, :noproc] and attempts > 1 ->
        Process.demonitor(ref, [:flush])
        join(room, participant, attempts - 1)
    else
      {:ok, snapshot, participant} ->
        {:ok, snapshot, participant, ref}

      error ->
        Process.demonitor(ref, [:flush])
        error
    end
  end

  def snapshot(room), do: call(room, :snapshot)

  @doc "Whether `pid` is `player_id`'s current connection."
  def current?(room, player_id, pid), do: call(room, {:current?, player_id, pid})

  @doc "Records the calling connection's seat (life, counters, deck, reveal)."
  def remember_seat(room, participant), do: call(room, {:remember_seat, participant})

  @doc "Reorders seats mid-game (Commander only); starts the clock if needed."
  def order(room, peer_ids), do: call(room, {:order, peer_ids})

  @doc "Arranges lobby seats before the game starts."
  def arrange(room, peer_ids), do: call(room, {:arrange, peer_ids})

  def mode(room, mode), do: call(room, {:mode, mode})

  def adjust_team_life(room, player_id, team_index, delta),
    do: call(room, {:team_life, player_id, team_index, delta})

  def eliminate(room, peer_id, eliminated), do: call(room, {:eliminate, peer_id, eliminated})

  def timer(room, action), do: call(room, {:timer, action})

  @doc "Starts the game; `randomize` overrides the room's auto-randomize setting."
  def start_game(room, randomize \\ nil), do: call(room, {:start_game, randomize})

  def turn_settings(room, auto_randomize), do: call(room, {:turn_settings, auto_randomize})

  def pass_turn(room, revision), do: call(room, {:pass_turn, revision})

  def adjust_turn(room, player_id, delta), do: call(room, {:adjust_turn, player_id, delta})

  def take_monarch(room, participant), do: call(room, {:monarch, participant})

  @doc "Applies a card list change on behalf of `actor`, the acting participant."
  def cards(room, payload, actor), do: call(room, {:cards, payload, actor})

  defp call(room, message), do: GenServer.call(Room.via(room), message)

  defp start_room(room) do
    case DynamicSupervisor.start_child(TheGathering.WebcamTables.RoomSupervisor, {Room, room}) do
      {:ok, pid} -> pid
      {:error, {:already_started, pid}} -> pid
    end
  end
end
