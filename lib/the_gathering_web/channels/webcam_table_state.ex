defmodule TheGatheringWeb.WebcamTableState do
  @moduledoc """
  Serialized, ephemeral turn order and timer state. Channel monitors discard a
  room when its last seat leaves. Broadcasts happen here to preserve update order.
  """
  use GenServer

  alias TheGatheringWeb.{Endpoint, WebcamTableTurns}

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  def join(room, pid, participant),
    do: GenServer.call(__MODULE__, {:join, room, pid, participant})

  def snapshot(room), do: GenServer.call(__MODULE__, {:snapshot, room})
  def order(room, peers), do: GenServer.call(__MODULE__, {:order, room, peers, true})
  def timer(room, action), do: GenServer.call(__MODULE__, {:timer, room, action})
  def start_game(room), do: GenServer.call(__MODULE__, {:start_game, room})

  def turn_settings(room, auto_randomize),
    do: GenServer.call(__MODULE__, {:turn_settings, room, auto_randomize})

  def pass_turn(room, revision), do: GenServer.call(__MODULE__, {:pass_turn, room, revision})

  def adjust_turn(room, player_id, delta),
    do: GenServer.call(__MODULE__, {:adjust_turn, room, player_id, delta})

  def remember_seat(room, participant),
    do: GenServer.call(__MODULE__, {:remember_seat, room, participant})

  def new_timer, do: %{started_at: nil, paused_at: nil, paused_ms: 0}

  def elapsed(%{started_at: nil}, _now), do: 0

  def elapsed(timer, now),
    do: max(0, (timer.paused_at || now) - timer.started_at - timer.paused_ms)

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
  def handle_call({:join, room, pid, participant}, _from, state) do
    ref = Process.monitor(pid)

    entry =
      Map.get(state.rooms, room, %{
        timer: new_timer(),
        peer_ids: [],
        members: MapSet.new(),
        eliminated_seats: %{},
        all_seats: %{},
        turns: WebcamTableTurns.new(),
        auto_randomize: true
      })

    entry = %{entry | members: MapSet.put(entry.members, ref)}
    previous = Map.get(entry.eliminated_seats, participant.player_id)

    # An eliminated player can leave without disappearing from the result. A
    # rejoin takes back that same seat, retaining its position and elimination.
    {entry, participant} =
      if previous do
        participant = %{previous | peer_id: participant.peer_id}
        previous_peer = previous.peer_id

        peers =
          Enum.map(entry.peer_ids, fn
            ^previous_peer -> participant.peer_id
            id -> id
          end)

        entry = %{
          entry
          | peer_ids: peers,
            eliminated_seats: Map.put(entry.eliminated_seats, participant.player_id, participant)
        }

        {entry, participant}
      else
        {entry, participant}
      end

    old_seat = Map.get(entry.all_seats, participant.player_id)

    peers =
      Enum.map(entry.peer_ids, fn id ->
        if old_seat && id == old_seat.peer_id, do: participant.peer_id, else: id
      end)

    entry = %{
      entry
      | peer_ids: peers,
        all_seats: Map.put(entry.all_seats, participant.player_id, participant)
    }

    entry = reconcile_turn(entry)

    state = %{
      state
      | rooms: Map.put(state.rooms, room, entry),
        monitors: Map.put(state.monitors, ref, {room, participant.player_id})
    }

    broadcast_state(room, entry)
    {:reply, {snapshot_entry(entry), participant}, state}
  end

  def handle_call({:snapshot, room}, _from, state) do
    {:reply, snapshot_entry(Map.fetch!(state.rooms, room)), state}
  end

  def handle_call({:remember_seat, room, participant}, _from, state) do
    entry = Map.fetch!(state.rooms, room)

    seats =
      if participant.eliminated,
        do: Map.put(entry.eliminated_seats, participant.player_id, participant),
        else: Map.delete(entry.eliminated_seats, participant.player_id)

    if seats != entry.eliminated_seats do
      Endpoint.broadcast!("webcam_table:#{room}", "eliminated_seats", %{
        participants: Map.values(seats)
      })
    end

    entry = %{
      entry
      | eliminated_seats: seats,
        all_seats: Map.put(entry.all_seats, participant.player_id, participant)
    }

    updated = reconcile_turn(entry)
    if updated.turns != entry.turns, do: broadcast_state(room, updated)
    {:reply, :ok, put_in(state, [:rooms, room], updated)}
  end

  def handle_call({:order, room, peers, shuffled}, _from, state) do
    entry = Map.fetch!(state.rooms, room)
    timer = update_timer(entry.timer, "start", System.system_time(:millisecond))
    # Keep departed eliminated seats in their recorded positions when live seats reshuffle.
    departed =
      entry.eliminated_seats
      |> Map.values()
      |> Enum.map(& &1.peer_id)
      |> Enum.reject(&(&1 in peers))

    {peers, remaining} =
      Enum.map_reduce(entry.peer_ids, peers, fn id, rest ->
        if id in departed, do: {id, rest}, else: {List.first(rest), Enum.drop(rest, 1)}
      end)

    peers =
      Enum.reject(peers, &is_nil/1) ++ remaining ++ Enum.reject(departed, &(&1 in entry.peer_ids))

    entry = reconcile_turn(%{entry | timer: timer, peer_ids: peers})

    Endpoint.broadcast!("webcam_table:#{room}", "seat_order", %{
      peer_ids: peers,
      shuffled: shuffled
    })

    broadcast_timer(room, entry)
    broadcast_state(room, entry)
    {:reply, :ok, put_in(state, [:rooms, room], entry)}
  end

  def handle_call({:timer, room, action}, _from, state) do
    entry = Map.fetch!(state.rooms, room)
    entry = %{entry | timer: update_timer(entry.timer, action, System.system_time(:millisecond))}
    snapshot = broadcast_timer(room, entry)
    {:reply, snapshot, put_in(state, [:rooms, room], entry)}
  end

  def handle_call({:start_game, room}, from, state) do
    entry = Map.fetch!(state.rooms, room)

    peers =
      entry
      |> ordered_seats()
      |> Enum.reject(&Map.get(&1, :departed, false))
      |> Enum.map(& &1.peer_id)

    peers = if entry.auto_randomize, do: Enum.shuffle(peers), else: peers
    handle_call({:order, room, peers, entry.auto_randomize}, from, state)
  end

  def handle_call({:turn_settings, room, enabled}, _from, state) do
    entry = %{Map.fetch!(state.rooms, room) | auto_randomize: enabled}
    broadcast_state(room, entry)
    {:reply, :ok, put_in(state, [:rooms, room], entry)}
  end

  def handle_call({:pass_turn, room, revision}, _from, state) do
    entry = Map.fetch!(state.rooms, room)

    if entry.turns.revision == revision and not is_nil(entry.turns.active_player_id) do
      turns =
        WebcamTableTurns.pass(
          entry.turns,
          ordered_seats(entry),
          elapsed(entry.timer, System.system_time(:millisecond))
        )

      entry = %{entry | turns: turns}
      broadcast_state(room, entry)
      {:reply, :ok, put_in(state, [:rooms, room], entry)}
    else
      {:reply, {:error, %{reason: "turn has changed or the game has not started"}}, state}
    end
  end

  def handle_call({:adjust_turn, room, player_id, delta}, _from, state) do
    entry = Map.fetch!(state.rooms, room)

    if Map.has_key?(entry.all_seats, player_id) do
      entry = %{entry | turns: WebcamTableTurns.adjust(entry.turns, player_id, delta)}
      broadcast_state(room, entry)
      {:reply, :ok, put_in(state, [:rooms, room], entry)}
    else
      {:reply, {:error, %{reason: "player is not in this game"}}, state}
    end
  end

  @impl true
  def handle_info({:DOWN, ref, :process, _pid, _reason}, state) do
    {{room, player_id}, monitors} = Map.pop(state.monitors, ref)
    entry = Map.fetch!(state.rooms, room)
    members = MapSet.delete(entry.members, ref)
    entry = update_in(entry, [:all_seats, player_id], &Map.put(&1, :departed, true))
    entry = reconcile_turn(entry)
    broadcast_state(room, entry)

    rooms =
      if MapSet.size(members) == 0,
        do: Map.delete(state.rooms, room),
        else: Map.put(state.rooms, room, %{entry | members: members})

    {:noreply, %{state | rooms: rooms, monitors: monitors}}
  end

  defp snapshot_entry(entry) do
    %{
      timer: Map.put(entry.timer, :server_now, System.system_time(:millisecond)),
      peer_ids: entry.peer_ids,
      eliminated_seats: Map.values(entry.eliminated_seats),
      turns: entry.turns,
      auto_randomize: entry.auto_randomize
    }
  end

  defp broadcast_timer(room, entry) do
    snapshot = snapshot_entry(entry).timer
    Endpoint.broadcast!("webcam_table:#{room}", "timer_state", snapshot)
    snapshot
  end

  defp broadcast_state(room, entry),
    do: Endpoint.broadcast!("webcam_table:#{room}", "table_state", snapshot_entry(entry))

  defp ordered_seats(entry) do
    positions = entry.peer_ids |> Enum.with_index() |> Map.new()

    entry.all_seats
    |> Map.values()
    |> Enum.sort_by(&{Map.get(positions, &1.peer_id, 999), &1.joined_at, &1.peer_id})
  end

  defp reconcile_turn(%{timer: %{started_at: nil}} = entry), do: entry

  defp reconcile_turn(entry) do
    turns =
      WebcamTableTurns.reconcile(
        entry.turns,
        ordered_seats(entry),
        elapsed(entry.timer, System.system_time(:millisecond))
      )

    %{entry | turns: turns}
  end
end
