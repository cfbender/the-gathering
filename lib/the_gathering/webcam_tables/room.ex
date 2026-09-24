defmodule TheGathering.WebcamTables.Room do
  @moduledoc """
  One webcam table's serialized admission and durable game state.

  Each room is its own process, registered by room id (with the time it opened
  as the registry value), so a busy or crashing room cannot stall or reset
  another. It loads the saved `Session` on start and keeps running after its
  last connection exits, until `close_if_idle/2` finds it empty and idle; then
  it deletes its session and stops. Presence describes connections, not seats:
  disconnects never change turns or erase a game. Every mutation is saved
  before it is broadcast and acknowledged, and broadcasts preserve update order.

  Connection processes receive `:seat_replaced` when a newer connection takes
  their seat and `{:seat_eliminated, boolean}` when their seat is knocked out
  or restored. Use the `TheGathering.WebcamTables` API rather than calling
  this module directly.
  """
  use GenServer, restart: :temporary

  alias TheGathering.WebcamTables.{Cards, Log, Session, Timer, Turns}
  alias TheGatheringWeb.Endpoint

  @max_seats 10
  # Keeps an idle but connected room (a long pause) from expiring.
  @refresh_interval :timer.hours(1)
  # A reload drops and rejoins within this window; only a longer absence is a leave.
  @departure_grace_ms 10_000

  def start_link(id) do
    name = {:via, Registry, {TheGathering.WebcamTables.Registry, id, now()}}
    GenServer.start_link(__MODULE__, id, name: name)
  end

  def via(id), do: {:via, Registry, {TheGathering.WebcamTables.Registry, id}}

  @doc """
  Deletes the room's session and stops it if no one is connected and nothing
  has happened for `idle_ms`. Returns `:closed` or `:open`.
  """
  def close_if_idle(pid, idle_ms), do: GenServer.call(pid, {:close_if_idle, idle_ms})

  @impl true
  def init(id) do
    Process.send_after(self(), :refresh, @refresh_interval)

    # `entry` stays nil for a brand-new room until its first join names the owner.
    # `active_at` is the last join, saved change or disconnect. `departing` holds
    # players whose last connection dropped within the grace period.
    {:ok,
     %{
       id: id,
       entry: Session.load(id),
       connections: %{},
       monitors: %{},
       departing: %{},
       active_at: now()
     }}
  end

  @impl true
  def handle_call({:join, participant}, {pid, _tag}, state) do
    entry = state.entry || new_entry(participant.player_id)
    previous = entry.all_seats[participant.player_id]

    cond do
      duplicate_peer?(entry, participant) ->
        {:reply, {:error, "peer id is already in use"}, state}

      lobby_full?(entry, previous) ->
        {:reply, {:error, "room is full"}, state}

      true ->
        admit(state, entry, previous, participant, pid)
    end
  end

  def handle_call({:current?, player_id, pid}, _from, state),
    do: {:reply, match?({^pid, _ref}, state.connections[player_id]), state}

  def handle_call(:snapshot, _from, state), do: {:reply, snapshot(state.entry), state}

  def handle_call(:log, _from, state), do: {:reply, state.entry.log, state}

  def handle_call({:roll, participant, roll}, _from, state) do
    roll =
      Map.merge(roll, %{
        id: Ecto.UUID.generate(),
        actor: participant.peer_id,
        player_name: participant.player_name,
        at: now()
      })

    entry = log(state.entry, [Log.roll(roll)])
    {:reply, :ok, commit(state, entry, [{"roll", roll}])}
  end

  def handle_call({:close_if_idle, idle_ms}, _from, state) do
    if map_size(state.connections) == 0 and now() - state.active_at >= idle_ms do
      :ok = Session.delete(state.id)
      {:stop, :normal, :closed, state}
    else
      {:reply, :open, state}
    end
  end

  def handle_call({:monarch, participant}, _from, %{entry: entry} = state) do
    holder = Map.take(participant, [:peer_id, :player_name])

    if holder == entry.monarch do
      {:reply, :ok, state}
    else
      entry = %{entry | monarch: holder, monarch_revision: entry.monarch_revision + 1}
      entry = log(entry, [Log.monarch(holder)])
      event = %{holder: holder, revision: entry.monarch_revision}
      {:reply, :ok, commit(state, entry, [{"monarch", event}])}
    end
  end

  def handle_call({:cards, payload, actor}, _from, %{entry: entry} = state) do
    case Cards.update(entry.cards, payload, Map.values(entry.all_seats)) do
      {:ok, cards} ->
        event = %{
          entries: cards,
          type: payload["type"],
          by: Map.take(actor, [:peer_id, :player_name])
        }

        {:reply, :ok, commit(state, %{entry | cards: cards}, [{"identified_cards", event}])}

      :error ->
        {:reply, {:error, %{reason: "invalid cards"}}, state}
    end
  end

  # Only the seat's current connection may update it, so a replaced tab's late
  # updates cannot overwrite the reloaded seat.
  def handle_call({:remember_seat, participant}, {pid, _tag}, %{entry: entry} = state) do
    if match?({^pid, _ref}, state.connections[participant.player_id]) do
      previous = entry.all_seats[participant.player_id]
      participant = %{participant | eliminated: previous.eliminated}
      eliminated_seats = put_eliminated(entry.eliminated_seats, participant)

      events =
        if eliminated_seats == entry.eliminated_seats,
          do: [:table_state],
          else: [eliminated_seats_event(eliminated_seats), :table_state]

      entry =
        reconcile_turn(%{
          entry
          | eliminated_seats: eliminated_seats,
            all_seats: Map.put(entry.all_seats, participant.player_id, participant)
        })

      entry = log(entry, Log.seat_changes(previous, participant, Map.values(entry.all_seats)))

      {:reply, :ok, commit(state, entry, events)}
    else
      {:reply, :ok, state}
    end
  end

  def handle_call({:order, peers}, _from, state),
    do: {:reply, :ok, reorder(state, state.entry, peers, false)}

  def handle_call({:arrange, peers}, _from, %{entry: entry} = state) do
    if is_nil(entry.timer.started_at),
      do: {:reply, :ok, commit(state, %{entry | peer_ids: peers})},
      else: {:reply, {:error, %{reason: "seat order is fixed after start"}}, state}
  end

  def handle_call({:mode, mode}, _from, %{entry: entry} = state) do
    if is_nil(entry.timer.started_at),
      do: {:reply, :ok, commit(state, %{entry | mode: mode, team_life: %{}})},
      else: {:reply, {:error, %{reason: "game mode is fixed after start"}}, state}
  end

  def handle_call({:team_life, player_id, team_index, delta}, _from, %{entry: entry} = state) do
    team = entry |> ordered_seats() |> Enum.chunk_every(2) |> Enum.at(team_index, [])

    if entry.mode == "two_headed_giant" and Map.has_key?(entry.team_life, team_index) and
         (entry.owner_id == player_id or Enum.any?(team, &(&1.player_id == player_id))) do
      life = entry.team_life[team_index] |> Kernel.+(delta) |> max(-999) |> min(999)
      entry = put_in(entry, [:team_life, team_index], life)

      # Zero shared life knocks the whole team out; restoring stays manual.
      if life <= 0 and Enum.any?(team, &(not &1.eliminated)) do
        entry = eliminate_seats(state, entry, team, true)
        events = [eliminated_seats_event(entry.eliminated_seats), :table_state]
        {:reply, :ok, commit(state, entry, events)}
      else
        {:reply, :ok, commit(state, entry)}
      end
    else
      {:reply,
       {:error, %{reason: "only teammates or the owner can change a started team's life"}}, state}
    end
  end

  def handle_call({:eliminate, peer_id, eliminated}, _from, %{entry: entry} = state) do
    seat = Enum.find(Map.values(entry.all_seats), &(&1.peer_id == peer_id))

    targets =
      if entry.mode == "two_headed_giant",
        do: Turns.team(ordered_seats(entry), seat.player_id),
        else: [seat]

    entry = eliminate_seats(state, entry, targets, eliminated)
    events = [eliminated_seats_event(entry.eliminated_seats), :table_state]
    {:reply, :ok, commit(state, entry, events)}
  end

  def handle_call({:timer, action}, _from, %{entry: entry} = state) do
    entry = %{entry | timer: Timer.update(entry.timer, action, now())}
    timer = timer_snapshot(entry)
    {:reply, timer, commit(state, entry, [{"timer_state", timer}])}
  end

  def handle_call({:start_game, randomize}, _from, %{entry: entry} = state) do
    randomize = if is_nil(randomize), do: entry.auto_randomize, else: randomize

    peers =
      entry
      |> ordered_seats()
      |> Enum.reject(&Map.get(&1, :departed, false))
      |> Enum.map(& &1.peer_id)

    cond do
      not is_nil(entry.timer.started_at) ->
        {:reply, :ok, state}

      reason = roster_error(entry.mode, length(peers)) ->
        {:reply, {:error, %{reason: reason}}, state}

      true ->
        team_life =
          if entry.mode == "two_headed_giant",
            do: Map.new(0..(div(length(peers), 2) - 1), &{&1, 60}),
            else: %{}

        peers = shuffle(peers, entry.mode, randomize)
        {:reply, :ok, reorder(state, %{entry | team_life: team_life}, peers, randomize)}
    end
  end

  def handle_call({:turn_settings, enabled}, _from, %{entry: entry} = state),
    do: {:reply, :ok, commit(state, %{entry | auto_randomize: enabled})}

  def handle_call({:pass_turn, revision}, _from, %{entry: entry} = state) do
    if entry.turns.revision == revision and not is_nil(entry.turns.active_player_id) do
      turns =
        Turns.pass(
          entry.turns,
          ordered_seats(entry),
          Timer.elapsed(entry.timer, now()),
          entry.mode
        )

      {:reply, :ok, commit(state, %{entry | turns: turns})}
    else
      {:reply, {:error, %{reason: "turn has changed or the game has not started"}}, state}
    end
  end

  def handle_call({:adjust_turn, player_id, delta}, _from, %{entry: entry} = state) do
    if Map.has_key?(entry.all_seats, player_id) do
      player_id = Turns.turn_id(ordered_seats(entry), player_id, entry.mode)
      turns = Turns.adjust(entry.turns, player_id, delta)
      {:reply, :ok, commit(state, %{entry | turns: turns})}
    else
      {:reply, {:error, %{reason: "player is not in this game"}}, state}
    end
  end

  @impl true
  def handle_info(:refresh, state) do
    if state.entry, do: :ok = Session.save(state.id, state.entry)
    Process.send_after(self(), :refresh, @refresh_interval)
    {:noreply, state}
  end

  def handle_info({:DOWN, ref, :process, _pid, _reason}, state) do
    case Map.pop(state.monitors, ref) do
      {nil, _monitors} ->
        {:noreply, state}

      {{player_id, name}, monitors} ->
        token = make_ref()
        Process.send_after(self(), {:departed, player_id, token}, @departure_grace_ms)

        {:noreply,
         %{
           state
           | monitors: monitors,
             connections: Map.delete(state.connections, player_id),
             departing: Map.put(state.departing, player_id, {name, token}),
             active_at: now()
         }}
    end
  end

  # Stale tokens belong to an earlier disconnect the player already returned from.
  def handle_info({:departed, player_id, token}, state) do
    case Map.pop(state.departing, player_id) do
      {{name, ^token}, departing} ->
        state = %{state | departing: departing}
        {:noreply, commit(state, log(state.entry, [Log.left(name)]), [])}

      _stale ->
        {:noreply, state}
    end
  end

  # Saves before broadcasting, so everything clients see is recoverable.
  # `broadcasts` are `:table_state` (the full snapshot) or `{event, payload}`;
  # new and merged log entries follow as `log_entry` events.
  defp commit(state, entry, broadcasts \\ [:table_state]) do
    :ok = Session.save(state.id, entry)
    log_entries = new_log_entries(state.entry && state.entry.log, entry.log)

    Enum.each(broadcasts ++ Enum.map(log_entries, &{"log_entry", &1}), fn
      :table_state -> broadcast!(state.id, "table_state", snapshot(entry))
      {event, payload} -> broadcast!(state.id, event, payload)
    end)

    %{state | entry: entry, active_at: now()}
  end

  defp log(entry, contents),
    do: %{entry | log: Enum.reduce(contents, entry.log, &Log.append(&2, &1, now()))}

  # Entries newer than the old head, plus the head itself if a merge changed it.
  defp new_log_entries(old, new) do
    head = List.first(old || [])

    new
    |> Enum.take_while(&(is_nil(head) or &1.id > head.id or (&1.id == head.id and &1 != head)))
    |> Enum.reverse()
  end

  defp broadcast!(id, event, payload),
    do: Endpoint.broadcast!("webcam_table:#{id}", event, payload)

  # Sets the seat order and starts the clock (idempotently). Shared by the
  # initial start and mid-game Commander reorders.
  defp reorder(state, entry, peers, shuffled) do
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

    # Cards identified in the lobby do not carry into the game.
    started? = not is_nil(entry.timer.started_at)
    cards = if started?, do: entry.cards, else: []
    timer = Timer.update(entry.timer, "start", now())
    entry = reconcile_turn(%{entry | timer: timer, peer_ids: peers, cards: cards})
    entry = log(entry, [Log.seat_order(shuffled, started?)])

    commit(state, entry, [
      {"seat_order", %{peer_ids: peers, shuffled: shuffled}},
      {"timer_state", timer_snapshot(entry)},
      :table_state
    ])
  end

  defp snapshot(entry) do
    %{
      timer: timer_snapshot(entry),
      peer_ids: entry.peer_ids,
      seats: Map.values(entry.all_seats),
      owner_id: entry.owner_id,
      monarch: %{holder: entry.monarch, revision: entry.monarch_revision},
      cards: entry.cards,
      eliminated_seats: Map.values(entry.eliminated_seats),
      turns: entry.turns,
      mode: entry.mode,
      team_life: entry.team_life,
      auto_randomize: entry.auto_randomize
    }
  end

  defp timer_snapshot(entry), do: Map.put(entry.timer, :server_now, now())

  defp eliminated_seats_event(eliminated_seats),
    do: {"eliminated_seats", %{participants: Map.values(eliminated_seats)}}

  defp new_entry(owner_id) do
    %{
      timer: Timer.new(),
      peer_ids: [],
      eliminated_seats: %{},
      all_seats: %{},
      turns: Turns.new(),
      mode: "commander",
      team_life: %{},
      auto_randomize: true,
      monarch: nil,
      monarch_revision: 0,
      cards: [],
      log: [],
      owner_id: owner_id
    }
  end

  defp duplicate_peer?(entry, participant) do
    Enum.any?(entry.all_seats, fn {id, seat} ->
      id != participant.player_id and seat.peer_id == participant.peer_id
    end)
  end

  defp lobby_full?(%{timer: %{started_at: nil}, all_seats: seats}, nil),
    do: map_size(seats) >= @max_seats

  defp lobby_full?(_entry, _previous), do: false

  # Seats the participant (a returning player gets their saved seat back; late
  # arrivals spectate) with `pid` as its connection.
  defp admit(state, entry, previous, participant, pid) do
    spectator? = is_nil(previous) and not is_nil(entry.timer.started_at)
    participant = if previous, do: %{previous | peer_id: participant.peer_id}, else: participant
    participant = Map.put(participant, :spectator, spectator?)

    # A reload replaces a live tab or returns within the grace period: not news.
    returning? =
      Map.has_key?(state.connections, participant.player_id) or
        Map.has_key?(state.departing, participant.player_id)

    state =
      replace_connection(
        %{state | departing: Map.delete(state.departing, participant.player_id)},
        pid,
        participant
      )

    entry = if spectator?, do: entry, else: restore_seat(entry, previous, participant)
    entry = if returning?, do: entry, else: log(entry, [Log.joined(participant.player_name)])
    {:reply, {:ok, snapshot(entry), participant}, commit(state, entry)}
  end

  # One live connection per player: a reload replaces the older tab. Monitors
  # remember the player's name so a later leave can be logged.
  defp replace_connection(state, pid, %{player_id: player_id, player_name: name}) do
    monitors =
      case state.connections[player_id] do
        nil ->
          state.monitors

        {old_pid, ref} ->
          Process.demonitor(ref, [:flush])
          send(old_pid, :seat_replaced)
          Map.delete(state.monitors, ref)
      end

    ref = Process.monitor(pid)

    %{
      state
      | connections: Map.put(state.connections, player_id, {pid, ref}),
        monitors: Map.put(monitors, ref, {player_id, name})
    }
  end

  # A returning player keeps their seat, position, crown and cards under the new peer id.
  defp restore_seat(entry, previous, participant) do
    replace_peer = fn id ->
      if previous && id == previous.peer_id, do: participant.peer_id, else: id
    end

    monarch = entry.monarch && %{entry.monarch | peer_id: replace_peer.(entry.monarch.peer_id)}

    cards =
      Enum.map(entry.cards, &Map.put(&1, "ownerPeerId", replace_peer.(&1["ownerPeerId"])))

    %{
      entry
      | peer_ids: Enum.map(entry.peer_ids, replace_peer),
        monarch: monarch,
        eliminated_seats: put_eliminated(entry.eliminated_seats, participant),
        cards: cards,
        all_seats: Map.put(entry.all_seats, participant.player_id, participant)
    }
  end

  defp put_eliminated(eliminated_seats, %{eliminated: true} = seat),
    do: Map.put(eliminated_seats, seat.player_id, seat)

  defp put_eliminated(eliminated_seats, seat),
    do: Map.delete(eliminated_seats, seat.player_id)

  defp ordered_seats(entry) do
    positions = entry.peer_ids |> Enum.with_index() |> Map.new()

    entry.all_seats
    |> Map.values()
    |> Enum.sort_by(&{Map.get(positions, &1.peer_id, 999), &1.joined_at, &1.peer_id})
  end

  defp roster_error("two_headed_giant", count) when count < 4 or rem(count, 2) != 0,
    do: "Two-Headed Giant requires an even number of players (at least 4)"

  defp roster_error("five_star", count) when count != 5,
    do: "Five Star requires exactly 5 players"

  defp roster_error(_mode, _count), do: nil

  defp shuffle(peers, _mode, false), do: peers

  defp shuffle(peers, "two_headed_giant", true),
    do: peers |> Enum.chunk_every(2) |> Enum.shuffle() |> List.flatten()

  defp shuffle(peers, _mode, true), do: Enum.shuffle(peers)

  # Marks seats in or out, tells their live connections, and moves the turn on
  # if the active seat just left. Callers commit the returned entry.
  defp eliminate_seats(state, entry, seats, eliminated) do
    entry =
      Enum.reduce(seats, entry, fn seat, entry ->
        entry =
          if seat.eliminated == eliminated,
            do: entry,
            else: log(entry, [Log.elimination(seat, eliminated)])

        seat = %{seat | eliminated: eliminated}

        case state.connections[seat.player_id] do
          {pid, _ref} -> send(pid, {:seat_eliminated, eliminated})
          nil -> :ok
        end

        %{
          entry
          | all_seats: Map.put(entry.all_seats, seat.player_id, seat),
            eliminated_seats: put_eliminated(entry.eliminated_seats, seat)
        }
      end)

    reconcile_turn(entry)
  end

  defp reconcile_turn(%{timer: %{started_at: nil}} = entry), do: entry

  defp reconcile_turn(entry) do
    turns =
      Turns.reconcile(
        entry.turns,
        ordered_seats(entry),
        Timer.elapsed(entry.timer, now()),
        entry.mode
      )

    %{entry | turns: turns}
  end

  defp now, do: System.system_time(:millisecond)
end
