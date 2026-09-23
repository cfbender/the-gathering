defmodule TheGatheringWeb.WebcamTableState do
  @moduledoc """
  Serialized, ephemeral turn order and timer state. Channel monitors discard a
  room when its last seat leaves. Broadcasts happen here to preserve update order.
  """
  use GenServer

  alias TheGatheringWeb.Endpoint

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  def join(room, pid), do: GenServer.call(__MODULE__, {:join, room, pid})
  def snapshot(room), do: GenServer.call(__MODULE__, {:snapshot, room})
  def order(room, peers), do: GenServer.call(__MODULE__, {:order, room, peers})
  def timer(room, action), do: GenServer.call(__MODULE__, {:timer, room, action})

  def new_timer, do: %{started_at: nil, paused_at: nil, paused_ms: 0}

  # Repeated actions are idempotent; randomizing again never resets or resumes.
  def update_timer(%{started_at: nil} = timer, "start", now),
    do: %{timer | started_at: now}

  def update_timer(%{started_at: nil} = timer, _action, _now), do: timer

  def update_timer(%{paused_at: nil} = timer, "pause", now),
    do: %{timer | paused_at: now}

  def update_timer(%{paused_at: paused} = timer, "resume", now) when not is_nil(paused),
    do: %{timer | paused_at: nil, paused_ms: timer.paused_ms + now - paused}

  def update_timer(timer, _action, _now), do: timer

  @impl true
  def init(_opts), do: {:ok, %{rooms: %{}, monitors: %{}}}

  @impl true
  def handle_call({:join, room, pid}, _from, state) do
    ref = Process.monitor(pid)
    entry = Map.get(state.rooms, room, %{timer: new_timer(), peer_ids: [], members: MapSet.new()})
    entry = %{entry | members: MapSet.put(entry.members, ref)}

    state = %{
      state
      | rooms: Map.put(state.rooms, room, entry),
        monitors: Map.put(state.monitors, ref, room)
    }

    {:reply, snapshot_entry(entry), state}
  end

  def handle_call({:snapshot, room}, _from, state) do
    {:reply, snapshot_entry(Map.fetch!(state.rooms, room)), state}
  end

  def handle_call({:order, room, peers}, _from, state) do
    entry = Map.fetch!(state.rooms, room)
    timer = update_timer(entry.timer, "start", System.system_time(:millisecond))
    entry = %{entry | timer: timer, peer_ids: peers}
    Endpoint.broadcast!("webcam_table:#{room}", "seat_order", %{peer_ids: peers})
    broadcast_timer(room, entry)
    {:reply, :ok, put_in(state, [:rooms, room], entry)}
  end

  def handle_call({:timer, room, action}, _from, state) do
    entry = Map.fetch!(state.rooms, room)
    entry = %{entry | timer: update_timer(entry.timer, action, System.system_time(:millisecond))}
    snapshot = broadcast_timer(room, entry)
    {:reply, snapshot, put_in(state, [:rooms, room], entry)}
  end

  @impl true
  def handle_info({:DOWN, ref, :process, _pid, _reason}, state) do
    {room, monitors} = Map.pop(state.monitors, ref)
    entry = Map.fetch!(state.rooms, room)
    members = MapSet.delete(entry.members, ref)

    rooms =
      if MapSet.size(members) == 0,
        do: Map.delete(state.rooms, room),
        else: Map.put(state.rooms, room, %{entry | members: members})

    {:noreply, %{state | rooms: rooms, monitors: monitors}}
  end

  defp snapshot_entry(entry) do
    %{
      timer: Map.put(entry.timer, :server_now, System.system_time(:millisecond)),
      peer_ids: entry.peer_ids
    }
  end

  defp broadcast_timer(room, entry) do
    snapshot = snapshot_entry(entry).timer
    Endpoint.broadcast!("webcam_table:#{room}", "timer_state", snapshot)
    snapshot
  end
end
