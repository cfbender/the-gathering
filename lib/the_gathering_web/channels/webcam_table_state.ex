defmodule TheGatheringWeb.WebcamTableState do
  @moduledoc """
  Serialized admission and durable game state. Presence describes connections,
  not seats: disconnects never change turns or erase a game. Broadcasts preserve
  update order and snapshots are committed before acknowledging mutations.
  """
  use GenServer

  alias TheGathering.WebcamTables.{Cards, Session}
  alias TheGatheringWeb.{Endpoint, WebcamTableTurns}

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  def join(room, pid, participant),
    do: GenServer.call(__MODULE__, {:join, room, pid, participant})

  def snapshot(room), do: GenServer.call(__MODULE__, {:snapshot, room})
  def order(room, peers), do: GenServer.call(__MODULE__, {:order, room, peers, true})
  def arrange(room, peers), do: GenServer.call(__MODULE__, {:arrange, room, peers})
  def mode(room, mode), do: GenServer.call(__MODULE__, {:mode, room, mode})

  def adjust_team_life(room, player_id, team, delta),
    do: GenServer.call(__MODULE__, {:team_life, room, player_id, team, delta})

  def eliminate(room, peer_id, eliminated),
    do: GenServer.call(__MODULE__, {:eliminate, room, peer_id, eliminated})

  def timer(room, action), do: GenServer.call(__MODULE__, {:timer, room, action})
  def start_game(room), do: GenServer.call(__MODULE__, {:start_game, room})

  def turn_settings(room, auto_randomize),
    do: GenServer.call(__MODULE__, {:turn_settings, room, auto_randomize})

  def pass_turn(room, revision), do: GenServer.call(__MODULE__, {:pass_turn, room, revision})

  def adjust_turn(room, player_id, delta),
    do: GenServer.call(__MODULE__, {:adjust_turn, room, player_id, delta})

  def remember_seat(room, participant),
    do: GenServer.call(__MODULE__, {:remember_seat, room, participant})

  def current?(room, player_id, pid),
    do: GenServer.call(__MODULE__, {:current, room, player_id, pid})

  def take_monarch(room, participant),
    do: GenServer.call(__MODULE__, {:monarch, room, participant})

  def cards(room, payload), do: GenServer.call(__MODULE__, {:cards, room, payload})

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
  def init(_opts) do
    Process.send_after(self(), :prune, :timer.hours(1))
    {:ok, %{rooms: %{}, monitors: %{}}}
  end

  @impl true
  def handle_call({:join, room, pid, participant}, _from, state) do
    entry = state.rooms[room] || load_entry(room, participant.player_id)

    previous = entry.all_seats[participant.player_id]

    cond do
      duplicate_peer?(entry, participant) ->
        {:reply, {:error, "peer id is already in use"}, state}

      lobby_full?(entry, previous) ->
        {:reply, {:error, "room is full"}, state}

      true ->
        spectator? = is_nil(previous) and not is_nil(entry.timer.started_at)

        participant =
          if previous, do: %{previous | peer_id: participant.peer_id}, else: participant

        participant = Map.put(participant, :spectator, spectator?)
        {entry, state} = replace_connection(entry, state, room, pid, participant)
        entry = if spectator?, do: entry, else: restore_seat(entry, previous, participant)
        Session.save(room, entry)
        broadcast_state(room, entry)
        {:reply, {:ok, snapshot_entry(entry), participant}, put_in(state, [:rooms, room], entry)}
    end
  end

  def handle_call({:current, room, player_id, pid}, _from, state) do
    current = get_in(state, [:rooms, room, :connections, player_id])
    {:reply, match?({^pid, _ref}, current), state}
  end

  def handle_call({:monarch, room, participant}, _from, state) do
    entry = Map.fetch!(state.rooms, room)
    holder = Map.take(participant, [:peer_id, :player_name])

    if holder == entry.monarch do
      {:reply, :ok, state}
    else
      entry = %{entry | monarch: holder, monarch_revision: entry.monarch_revision + 1}
      Session.save(room, entry)

      Endpoint.broadcast!("webcam_table:#{room}", "monarch", %{
        holder: holder,
        revision: entry.monarch_revision
      })

      {:reply, :ok, put_in(state, [:rooms, room], entry)}
    end
  end

  def handle_call({:snapshot, room}, _from, state) do
    {:reply, snapshot_entry(Map.fetch!(state.rooms, room)), state}
  end

  def handle_call({:cards, room, payload}, _from, state) do
    entry = Map.fetch!(state.rooms, room)

    case Cards.update(entry.cards, payload, Map.values(entry.all_seats)) do
      {:ok, cards} ->
        entry = %{entry | cards: cards}
        Session.save(room, entry)
        Endpoint.broadcast!("webcam_table:#{room}", "identified_cards", %{entries: cards})
        {:reply, :ok, put_in(state, [:rooms, room], entry)}

      :error ->
        {:reply, {:error, %{reason: "invalid cards"}}, state}
    end
  end

  def handle_call({:remember_seat, room, participant}, {pid, _tag}, state) do
    entry = Map.fetch!(state.rooms, room)

    if match?({^pid, _ref}, entry.connections[participant.player_id]) do
      participant = %{participant | eliminated: entry.all_seats[participant.player_id].eliminated}

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
      Session.save(room, updated)
      broadcast_state(room, updated)
      {:reply, :ok, put_in(state, [:rooms, room], updated)}
    else
      {:reply, :ok, state}
    end
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

    cards = if is_nil(entry.timer.started_at), do: [], else: entry.cards
    entry = reconcile_turn(%{entry | timer: timer, peer_ids: peers, cards: cards})
    Session.save(room, entry)

    Endpoint.broadcast!("webcam_table:#{room}", "seat_order", %{
      peer_ids: peers,
      shuffled: shuffled
    })

    broadcast_timer(room, entry)
    broadcast_state(room, entry)
    {:reply, :ok, put_in(state, [:rooms, room], entry)}
  end

  def handle_call({:arrange, room, peers}, _from, state) do
    entry = Map.fetch!(state.rooms, room)

    if is_nil(entry.timer.started_at) do
      entry = %{entry | peer_ids: peers}
      Session.save(room, entry)
      broadcast_state(room, entry)
      {:reply, :ok, put_in(state, [:rooms, room], entry)}
    else
      {:reply, {:error, %{reason: "seat order is fixed after start"}}, state}
    end
  end

  def handle_call({:mode, room, mode}, _from, state) do
    entry = Map.fetch!(state.rooms, room)

    if is_nil(entry.timer.started_at) do
      entry = %{entry | mode: mode, team_life: %{}}
      Session.save(room, entry)
      broadcast_state(room, entry)
      {:reply, :ok, put_in(state, [:rooms, room], entry)}
    else
      {:reply, {:error, %{reason: "game mode is fixed after start"}}, state}
    end
  end

  def handle_call({:team_life, room, player_id, team_index, delta}, _from, state) do
    entry = Map.fetch!(state.rooms, room)
    team = entry |> ordered_seats() |> Enum.chunk_every(2) |> Enum.at(team_index, [])

    if entry.mode == "two_headed_giant" and Map.has_key?(entry.team_life, team_index) and
         (entry.owner_id == player_id or Enum.any?(team, &(&1.player_id == player_id))) do
      life = entry.team_life[team_index] |> Kernel.+(delta) |> max(-999) |> min(999)
      entry = put_in(entry, [:team_life, team_index], life)
      Session.save(room, entry)
      broadcast_state(room, entry)
      {:reply, :ok, put_in(state, [:rooms, room], entry)}
    else
      {:reply,
       {:error, %{reason: "only teammates or the owner can change a started team's life"}}, state}
    end
  end

  def handle_call({:eliminate, room, peer_id, eliminated}, _from, state) do
    entry = Map.fetch!(state.rooms, room)
    seat = Enum.find(Map.values(entry.all_seats), &(&1.peer_id == peer_id))

    targets =
      if entry.mode == "two_headed_giant",
        do: WebcamTableTurns.team(ordered_seats(entry), seat.player_id),
        else: [seat]

    entry =
      Enum.reduce(targets, entry, fn seat, entry ->
        seat = %{seat | eliminated: eliminated}

        case entry.connections[seat.player_id] do
          {pid, _ref} -> send(pid, {:seat_eliminated, eliminated})
          nil -> :ok
        end

        eliminated_seats =
          if eliminated,
            do: Map.put(entry.eliminated_seats, seat.player_id, seat),
            else: Map.delete(entry.eliminated_seats, seat.player_id)

        %{
          entry
          | all_seats: Map.put(entry.all_seats, seat.player_id, seat),
            eliminated_seats: eliminated_seats
        }
      end)

    entry = reconcile_turn(entry)
    Session.save(room, entry)

    Endpoint.broadcast!("webcam_table:#{room}", "eliminated_seats", %{
      participants: Map.values(entry.eliminated_seats)
    })

    broadcast_state(room, entry)
    {:reply, :ok, put_in(state, [:rooms, room], entry)}
  end

  def handle_call({:timer, room, action}, _from, state) do
    entry = Map.fetch!(state.rooms, room)
    entry = %{entry | timer: update_timer(entry.timer, action, System.system_time(:millisecond))}
    Session.save(room, entry)
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

    cond do
      not is_nil(entry.timer.started_at) ->
        {:reply, :ok, state}

      entry.mode == "two_headed_giant" and (length(peers) < 4 or rem(length(peers), 2) != 0) ->
        {:reply,
         {:error, %{reason: "Two-Headed Giant requires an even number of players (at least 4)"}},
         state}

      entry.mode == "five_star" and length(peers) != 5 ->
        {:reply, {:error, %{reason: "Five Star requires exactly 5 players"}}, state}

      true ->
        peers = shuffle(peers, entry)

        life =
          if entry.mode == "two_headed_giant",
            do: Map.new(0..(div(length(peers), 2) - 1), &{&1, 60}),
            else: %{}

        state = put_in(state, [:rooms, room, :team_life], life)
        handle_call({:order, room, peers, entry.auto_randomize}, from, state)
    end
  end

  def handle_call({:turn_settings, room, enabled}, _from, state) do
    entry = %{Map.fetch!(state.rooms, room) | auto_randomize: enabled}
    Session.save(room, entry)
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
          elapsed(entry.timer, System.system_time(:millisecond)),
          entry.mode
        )

      entry = %{entry | turns: turns}
      Session.save(room, entry)
      broadcast_state(room, entry)
      {:reply, :ok, put_in(state, [:rooms, room], entry)}
    else
      {:reply, {:error, %{reason: "turn has changed or the game has not started"}}, state}
    end
  end

  def handle_call({:adjust_turn, room, player_id, delta}, _from, state) do
    entry = Map.fetch!(state.rooms, room)

    if Map.has_key?(entry.all_seats, player_id) do
      player_id = WebcamTableTurns.turn_id(ordered_seats(entry), player_id, entry.mode)
      entry = %{entry | turns: WebcamTableTurns.adjust(entry.turns, player_id, delta)}
      Session.save(room, entry)
      broadcast_state(room, entry)
      {:reply, :ok, put_in(state, [:rooms, room], entry)}
    else
      {:reply, {:error, %{reason: "player is not in this game"}}, state}
    end
  end

  @impl true
  def handle_info(:prune, state) do
    # Refresh connected rooms, even when the game is paused and nobody clicks.
    Enum.each(state.rooms, fn {room, entry} -> Session.save(room, entry) end)
    Session.prune()
    Process.send_after(self(), :prune, :timer.hours(1))
    {:noreply, state}
  end

  def handle_info({:DOWN, ref, :process, _pid, _reason}, state) do
    case Map.pop(state.monitors, ref) do
      {nil, _} ->
        {:noreply, state}

      {{room, player_id}, monitors} ->
        entry = state.rooms[room]
        entry = %{entry | connections: Map.delete(entry.connections, player_id)}

        rooms =
          if map_size(entry.connections) == 0,
            do: Map.delete(state.rooms, room),
            else: Map.put(state.rooms, room, entry)

        {:noreply, %{state | rooms: rooms, monitors: monitors}}
    end
  end

  defp snapshot_entry(entry) do
    %{
      timer: Map.put(entry.timer, :server_now, System.system_time(:millisecond)),
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

  defp broadcast_timer(room, entry) do
    snapshot = snapshot_entry(entry).timer
    Endpoint.broadcast!("webcam_table:#{room}", "timer_state", snapshot)
    snapshot
  end

  defp broadcast_state(room, entry),
    do: Endpoint.broadcast!("webcam_table:#{room}", "table_state", snapshot_entry(entry))

  defp load_entry(room, owner_id) do
    entry =
      Session.load(room) ||
        %{
          timer: new_timer(),
          peer_ids: [],
          eliminated_seats: %{},
          all_seats: %{},
          turns: WebcamTableTurns.new(),
          mode: "commander",
          team_life: %{},
          auto_randomize: true,
          monarch: nil,
          monarch_revision: 0,
          cards: [],
          owner_id: owner_id
        }

    Map.put(entry, :connections, %{})
  end

  defp duplicate_peer?(entry, participant) do
    Enum.any?(entry.all_seats, fn {id, seat} ->
      id != participant.player_id and seat.peer_id == participant.peer_id
    end)
  end

  defp lobby_full?(%{timer: %{started_at: nil}, all_seats: seats}, nil),
    do: map_size(seats) >= 10

  defp lobby_full?(_entry, _previous), do: false

  defp replace_connection(entry, state, room, pid, participant) do
    state =
      case entry.connections[participant.player_id] do
        nil ->
          state

        {old_pid, ref} ->
          Process.demonitor(ref, [:flush])
          send(old_pid, :seat_replaced)
          %{state | monitors: Map.delete(state.monitors, ref)}
      end

    ref = Process.monitor(pid)
    entry = put_in(entry, [:connections, participant.player_id], {pid, ref})
    state = put_in(state, [:monitors, ref], {room, participant.player_id})
    {entry, state}
  end

  defp restore_seat(entry, previous, participant) do
    peers =
      Enum.map(entry.peer_ids, fn id ->
        if previous && id == previous.peer_id, do: participant.peer_id, else: id
      end)

    monarch =
      if previous && entry.monarch && entry.monarch.peer_id == previous.peer_id,
        do: %{entry.monarch | peer_id: participant.peer_id},
        else: entry.monarch

    eliminated =
      if participant.eliminated,
        do: Map.put(entry.eliminated_seats, participant.player_id, participant),
        else: Map.delete(entry.eliminated_seats, participant.player_id)

    cards =
      Enum.map(entry.cards, fn card ->
        if previous && card["ownerPeerId"] == previous.peer_id,
          do: Map.put(card, "ownerPeerId", participant.peer_id),
          else: card
      end)

    %{
      entry
      | peer_ids: peers,
        monarch: monarch,
        eliminated_seats: eliminated,
        cards: cards,
        all_seats: Map.put(entry.all_seats, participant.player_id, participant)
    }
  end

  defp ordered_seats(entry) do
    positions = entry.peer_ids |> Enum.with_index() |> Map.new()

    entry.all_seats
    |> Map.values()
    |> Enum.sort_by(&{Map.get(positions, &1.peer_id, 999), &1.joined_at, &1.peer_id})
  end

  defp shuffle(peers, %{auto_randomize: false}), do: peers

  defp shuffle(peers, %{mode: "two_headed_giant"}),
    do: peers |> Enum.chunk_every(2) |> Enum.shuffle() |> List.flatten()

  defp shuffle(peers, _entry), do: Enum.shuffle(peers)

  defp reconcile_turn(%{timer: %{started_at: nil}} = entry), do: entry

  defp reconcile_turn(entry) do
    turns =
      WebcamTableTurns.reconcile(
        entry.turns,
        ordered_seats(entry),
        elapsed(entry.timer, System.system_time(:millisecond)),
        entry.mode
      )

    %{entry | turns: turns}
  end
end
