defmodule TheGatheringWeb.WebcamTableChannel do
  @moduledoc false

  use TheGatheringWeb, :channel

  alias TheGathering.Games
  alias TheGatheringWeb.{ChannelRateLimit, Presence, WebcamTableRooms, WebcamTableState}

  @starting_life 40
  @life_range -999..999
  # SDP offers with many candidates run 10–20 KB; anything far larger is abuse,
  # since every signal fans out to the whole room.
  @max_signal_bytes 65_536

  intercept ["presence_diff"]

  @impl true
  def join("webcam_table:" <> room_id, params, socket) do
    with :ok <- join_rate_limit(socket.assigns.user.id),
         true <- valid_room_id?(room_id),
         {:ok, participant} <- participant(params, socket.assigns.user.id),
         {:ok, state, participant} <- WebcamTableState.join(room_id, self(), participant) do
      send(self(), :after_join)

      {:ok, %{participant: participant, table_state: state},
       socket
       |> assign(:participant, participant)
       |> assign(:room_id, room_id)
       |> assign(:rate_limits, %{
         events: ChannelRateLimit.new(:webcam_table_events),
         signals: ChannelRateLimit.new(:webcam_table_signals)
       })
       |> assign(:owner?, state.owner_id == participant.player_id)
       |> assign(:protocol, Map.get(params, "protocol", 1))}
    else
      false -> {:error, %{reason: "room is full or invalid"}}
      {:error, reason} -> {:error, %{reason: reason}}
    end
  end

  @impl true
  def handle_info(:after_join, socket) do
    participant = socket.assigns.participant
    {:ok, _ref} = Presence.track(socket, participant.peer_id, participant)

    unless participant.spectator,
      do: WebcamTableRooms.track_seat(socket.assigns.room_id, participant)

    state = WebcamTableState.snapshot(socket.assigns.room_id)
    push(socket, "table_state", state)
    push(socket, "presence_state", Presence.list(socket))
    push(socket, "monarch_state", state.monarch)
    {:noreply, assign(socket, :participant, participant)}
  end

  def handle_info(:seat_replaced, socket) do
    push(socket, "seat_replaced", %{})
    {:stop, :normal, socket}
  end

  def handle_info({:seat_eliminated, eliminated}, socket) do
    participant = %{socket.assigns.participant | eliminated: eliminated}
    {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
    {:noreply, assign(socket, :participant, participant)}
  end

  def handle_info({:monarch_state, event}, socket) do
    push(socket, "monarch_state", event)
    {:noreply, socket}
  end

  @impl true
  def handle_out("presence_diff", diff, socket) do
    target = socket.assigns.participant.reveal_to

    socket =
      if target && Map.has_key?(diff.leaves, target) &&
           not Map.has_key?(Presence.list(socket), target) do
        put_reveal(socket, nil)
      else
        socket
      end

    push(socket, "presence_diff", diff)
    {:noreply, socket}
  end

  # Every event spends a token before it is handled, so floods are refused
  # before they validate, broadcast or write SQLite. Signals have their own,
  # larger bucket because connecting to a full table sends dozens at once.
  @impl true
  def handle_in(event, payload, socket) do
    bucket = if event == "signal", do: :signals, else: :events

    case ChannelRateLimit.take(socket.assigns.rate_limits[bucket]) do
      {:ok, updated} ->
        rate_limits = Map.put(socket.assigns.rate_limits, bucket, updated)
        handle_event(event, payload, assign(socket, :rate_limits, rate_limits))

      {:error, :rate_limited} ->
        {:reply, {:error, %{reason: "rate limited"}}, socket}
    end
  end

  defp handle_event(event, _payload, %{assigns: %{participant: %{spectator: true}}} = socket)
       when event not in ["signal", "timer_sync"] do
    {:reply, {:error, %{reason: "spectators cannot change the game"}}, socket}
  end

  # Old clients republish defaults immediately after joining. Ignore that one
  # legacy status echo rather than overwriting the restored durable seat.
  defp handle_event(
         "update_status",
         %{
           "life" => _,
           "poison" => _,
           "rad" => _,
           "commander_casts" => _,
           "commander_damage" => _,
           "camera_off" => _
         },
         %{assigns: %{protocol: 1}} = socket
       ) do
    {:reply, :ok, assign(socket, :protocol, 2)}
  end

  defp handle_event(event, _payload, %{assigns: %{owner?: false}} = socket)
       when event in [
              "start_game",
              "seat_order",
              "arrange_seats",
              "set_mode",
              "turn_settings",
              "adjust_turn",
              "timer"
            ] do
    {:reply, {:error, %{reason: "only the room owner can change table controls"}}, socket}
  end

  defp handle_event(
         "cards",
         %{"type" => "cards_cleared", "ownerPeerId" => owner} = payload,
         socket
       ) do
    if owner == socket.assigns.participant.peer_id,
      do: {:reply, update_cards(socket, payload), socket},
      else: {:reply, {:error, %{reason: "only the board owner can clear its cards"}}, socket}
  end

  # Attribution is always the sender's seat; a client-supplied name is ignored.
  defp handle_event("cards", %{"type" => "card_identified", "entry" => entry} = payload, socket)
       when is_map(entry) do
    entry = Map.put(entry, "byPlayerName", socket.assigns.participant.player_name)
    {:reply, update_cards(socket, %{payload | "entry" => entry}), socket}
  end

  # Any seated player may remove any entry to correct a misidentification; the
  # broadcast records who did it.
  defp handle_event("cards", payload, socket) do
    {:reply, update_cards(socket, payload), socket}
  end

  defp handle_event("signal", %{"target" => target, "signal" => signal}, socket)
       when is_binary(target) and is_map(signal) do
    cond do
      not uuid?(target) ->
        {:reply, {:error, %{reason: "invalid signal"}}, socket}

      byte_size(Jason.encode!(signal)) > @max_signal_bytes ->
        {:reply, {:error, %{reason: "signal too large"}}, socket}

      true ->
        broadcast_from!(socket, "signal", %{
          target: target,
          from: socket.assigns.participant.peer_id,
          signal: signal
        })

        {:noreply, socket}
    end
  end

  defp handle_event("signal", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid signal"}}, socket}

  defp handle_event("choose_deck", %{"deck_id" => deck_id}, socket) when is_integer(deck_id) do
    participant = socket.assigns.participant

    case Games.get_deck(deck_id) do
      %{player_id: player_id} = deck when player_id == participant.player_id ->
        participant = Map.merge(participant, %{deck_id: deck.id, deck_name: deck.name})
        {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
        # Peers may have cached the deck list before this deck was created or edited.
        broadcast!(socket, "deck_selected", %{deck_id: deck.id})
        WebcamTableState.remember_seat(socket.assigns.room_id, participant)
        {:reply, :ok, assign(socket, :participant, participant)}

      _other ->
        {:reply, {:error, %{reason: "deck does not belong to player"}}, socket}
    end
  end

  defp handle_event("choose_deck", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid deck"}}, socket}

  # Ephemeral state a player publishes about their own seat, carried by presence.
  defp handle_event("reveal", %{"target" => target} = payload, socket)
       when map_size(payload) == 1 and (is_binary(target) or is_nil(target)) do
    if is_nil(target) or
         (target != socket.assigns.participant.peer_id and
            Map.has_key?(Presence.list(socket), target)) do
      {:reply, :ok, put_reveal(socket, target)}
    else
      {:reply, {:error, %{reason: "reveal target must be another seated player"}}, socket}
    end
  end

  defp handle_event("reveal", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid reveal"}}, socket}

  # Ephemeral table state a player publishes about their own seat: life total and
  # whether their camera is off. It rides on presence like the deck.
  defp handle_event("update_status", payload, socket) when is_map(payload) do
    case status_changes(payload) do
      {:ok, changes} ->
        changes = eliminate_at_zero(changes, socket.assigns.participant)
        participant = Map.merge(socket.assigns.participant, changes)
        {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
        WebcamTableState.remember_seat(socket.assigns.room_id, participant)

        if Map.has_key?(changes, :eliminated),
          do:
            WebcamTableState.eliminate(
              socket.assigns.room_id,
              participant.peer_id,
              changes.eliminated
            )

        {:reply, :ok, assign(socket, :participant, participant)}

      :error ->
        {:reply, {:error, %{reason: "invalid status"}}, socket}
    end
  end

  defp handle_event("update_status", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid status"}}, socket}

  defp handle_event("take_monarch", payload, socket) when payload == %{} do
    :ok = WebcamTableState.take_monarch(socket.assigns.room_id, socket.assigns.participant)
    {:reply, :ok, socket}
  end

  defp handle_event("take_monarch", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid monarch claim"}}, socket}

  # Any seated player may eliminate/restore a present seat. The target channel
  # owns its presence update, so subsequent life/camera updates cannot overwrite it.
  defp handle_event(
         "set_eliminated",
         %{"peer_id" => peer_id, "eliminated" => eliminated} = payload,
         socket
       )
       when map_size(payload) == 2 and is_binary(peer_id) and is_boolean(eliminated) do
    if (socket.assigns.owner? or peer_id == socket.assigns.participant.peer_id) and
         Enum.any?(
           WebcamTableState.snapshot(socket.assigns.room_id).seats,
           &(&1.peer_id == peer_id)
         ) and
         Map.has_key?(Presence.list(socket), peer_id) do
      WebcamTableState.eliminate(socket.assigns.room_id, peer_id, eliminated)

      {:reply, :ok, socket}
    else
      {:reply, {:error, %{reason: "player must be present to change elimination"}}, socket}
    end
  end

  defp handle_event("set_eliminated", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid elimination"}}, socket}

  # Seat order is shared so every browser records the same turn order. The
  # proposed order must name exactly the peers present at that moment.
  defp handle_event(event, %{"peer_ids" => peer_ids}, socket)
       when event in ["seat_order", "arrange_seats"] and is_list(peer_ids) do
    state = WebcamTableState.snapshot(socket.assigns.room_id)

    present =
      state.seats
      |> Enum.map(& &1.peer_id)
      |> Enum.sort()

    if Enum.all?(peer_ids, &is_binary/1) and Enum.sort(peer_ids) == present do
      reply =
        if event == "arrange_seats" or state.mode != "commander",
          do: WebcamTableState.arrange(socket.assigns.room_id, peer_ids),
          else: WebcamTableState.order(socket.assigns.room_id, peer_ids)

      {:reply, reply, socket}
    else
      {:reply, {:error, %{reason: "seat order must list every seated player"}}, socket}
    end
  end

  defp handle_event(event, _payload, socket) when event in ["seat_order", "arrange_seats"],
    do: {:reply, {:error, %{reason: "invalid seat order"}}, socket}

  defp handle_event("set_mode", %{"mode" => mode} = payload, socket)
       when map_size(payload) == 1 and mode in ["commander", "two_headed_giant", "five_star"] do
    {:reply, WebcamTableState.mode(socket.assigns.room_id, mode), socket}
  end

  defp handle_event("set_mode", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid game mode"}}, socket}

  defp handle_event(
         "adjust_team_life",
         %{"team_index" => team, "delta" => delta} = payload,
         socket
       )
       when map_size(payload) == 2 and is_integer(team) and team >= 0 and is_integer(delta) and
              delta in -1998..1998 do
    {:reply,
     WebcamTableState.adjust_team_life(
       socket.assigns.room_id,
       socket.assigns.participant.player_id,
       team,
       delta
     ), socket}
  end

  defp handle_event("adjust_team_life", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid team life adjustment"}}, socket}

  # An explicit `randomize` overrides the room's auto-randomize setting for this start.
  defp handle_event("start_game", payload, socket) when payload == %{} do
    {:reply, WebcamTableState.start_game(socket.assigns.room_id), socket}
  end

  defp handle_event("start_game", %{"randomize" => randomize} = payload, socket)
       when map_size(payload) == 1 and is_boolean(randomize) do
    {:reply, WebcamTableState.start_game(socket.assigns.room_id, randomize), socket}
  end

  defp handle_event("start_game", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid start"}}, socket}

  defp handle_event("turn_settings", %{"auto_randomize" => enabled} = payload, socket)
       when map_size(payload) == 1 and is_boolean(enabled) do
    {:reply, WebcamTableState.turn_settings(socket.assigns.room_id, enabled), socket}
  end

  defp handle_event("turn_settings", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid turn settings"}}, socket}

  defp handle_event("pass_turn", %{"revision" => revision} = payload, socket)
       when map_size(payload) == 1 and is_integer(revision) and revision >= 0 do
    {:reply, WebcamTableState.pass_turn(socket.assigns.room_id, revision), socket}
  end

  defp handle_event("pass_turn", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid pass turn"}}, socket}

  defp handle_event(
         "adjust_turn",
         %{"player_id" => player_id, "delta" => delta} = payload,
         socket
       )
       when map_size(payload) == 2 and is_integer(player_id) and delta in [-1, 1] do
    {:reply, WebcamTableState.adjust_turn(socket.assigns.room_id, player_id, delta), socket}
  end

  defp handle_event("adjust_turn", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid turn adjustment"}}, socket}

  defp handle_event("timer", %{"action" => action} = payload, socket)
       when map_size(payload) == 1 and action in ["pause", "resume"] do
    timer = WebcamTableState.timer(socket.assigns.room_id, action)
    {:reply, {:ok, timer}, socket}
  end

  defp handle_event("timer", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid timer action"}}, socket}

  defp handle_event("timer_sync", payload, socket) when payload == %{} do
    {:reply, {:ok, WebcamTableState.snapshot(socket.assigns.room_id).timer}, socket}
  end

  defp handle_event("timer_sync", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid timer sync"}}, socket}

  defp handle_event("roll", %{"kind" => "dice", "sides" => sides} = payload, socket)
       when map_size(payload) == 2 and is_integer(sides) and sides in 2..1000 do
    broadcast_roll(socket, %{kind: "dice", sides: sides, result: :rand.uniform(sides)})
    {:reply, :ok, socket}
  end

  defp handle_event("roll", %{"kind" => "coin"} = payload, socket) when map_size(payload) == 1 do
    broadcast_roll(socket, %{kind: "coin", result: Enum.random(["Heads", "Tails"])})
    {:reply, :ok, socket}
  end

  defp handle_event("roll", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid roll (dice must have 2–1000 sides)"}}, socket}

  defp join_rate_limit(user_id) do
    case ChannelRateLimit.join(user_id) do
      :ok -> :ok
      {:error, :rate_limited} -> {:error, "rate limited"}
    end
  end

  defp update_cards(socket, payload),
    do: WebcamTableState.cards(socket.assigns.room_id, payload, socket.assigns.participant)

  defp put_reveal(socket, target) do
    participant = %{socket.assigns.participant | reveal_to: target}
    {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
    WebcamTableState.remember_seat(socket.assigns.room_id, participant)
    assign(socket, :participant, participant)
  end

  defp broadcast_roll(socket, roll) do
    participant = socket.assigns.participant

    broadcast!(
      socket,
      "roll",
      Map.merge(roll, %{
        id: Ecto.UUID.generate(),
        actor: participant.peer_id,
        player_name: participant.player_name,
        at: System.system_time(:millisecond)
      })
    )
  end

  defp status_changes(payload) do
    Enum.reduce_while(payload, {:ok, %{}}, fn
      {"life", life}, {:ok, changes} when is_integer(life) and life in @life_range ->
        {:cont, {:ok, Map.put(changes, :life, life)}}

      {"camera_off", camera_off}, {:ok, changes} when is_boolean(camera_off) ->
        {:cont, {:ok, Map.put(changes, :camera_off, camera_off)}}

      {key, count}, {:ok, changes}
      when key in ["poison", "rad"] and is_integer(count) and count in 0..999 ->
        field = if key == "poison", do: :poison, else: :rad
        {:cont, {:ok, Map.put(changes, field, count)}}

      {"commander_casts", counts}, {:ok, changes} ->
        if valid_counts?(counts),
          do: {:cont, {:ok, Map.put(changes, :commander_casts, counts)}},
          else: {:halt, :error}

      {"commander_damage", damage}, {:ok, changes} ->
        if valid_damage?(damage),
          do: {:cont, {:ok, Map.put(changes, :commander_damage, damage)}},
          else: {:halt, :error}

      {"eliminated", eliminated}, {:ok, changes} when is_boolean(eliminated) ->
        {:cont, {:ok, Map.put(changes, :eliminated, eliminated)}}

      _invalid, _changes ->
        {:halt, :error}
    end)
  end

  # Dropping to zero life knocks a player out in every format. Restoring is
  # deliberately manual, so gaining life back does not silently un-eliminate.
  defp eliminate_at_zero(%{life: life} = changes, %{eliminated: false})
       when life <= 0 and not is_map_key(changes, :eliminated),
       do: Map.put(changes, :eliminated, true)

  defp eliminate_at_zero(changes, _participant), do: changes

  defp valid_counts?(counts) when is_map(counts) and map_size(counts) <= 100 do
    Enum.all?(counts, fn {name, count} ->
      is_binary(name) and byte_size(name) in 1..300 and
        is_integer(count) and count in 0..999
    end)
  end

  defp valid_counts?(_counts), do: false

  defp valid_damage?(damage) when is_map(damage) and map_size(damage) <= 100 do
    Enum.all?(damage, fn {player_id, counts} ->
      is_binary(player_id) and Regex.match?(~r/^[1-9][0-9]{0,15}$/, player_id) and
        valid_counts?(counts)
    end)
  end

  defp valid_damage?(_damage), do: false

  defp participant(%{"peer_id" => peer_id, "player_id" => player_id} = params, user_id)
       when is_integer(player_id) do
    case uuid?(peer_id) && Games.get_player(player_id) do
      false ->
        {:error, "invalid peer id"}

      %{user_id: ^user_id} = player ->
        participant = %{
          peer_id: peer_id,
          player_id: player.id,
          player_name: player.name,
          life: @starting_life,
          camera_off: false,
          poison: 0,
          rad: 0,
          commander_casts: %{},
          commander_damage: %{},
          reveal_to: nil,
          eliminated: false,
          # Default seat order is join order, so every browser sees the same seats.
          joined_at: System.system_time(:millisecond)
        }

        {:ok, maybe_put_deck(participant, Map.get(params, "deck_id"))}

      _unlinked_or_other_player ->
        {:error, "account is not linked to this player"}
    end
  end

  defp participant(_params, _user_id), do: {:error, "account is not linked to a player"}

  # Clients generate peer IDs with crypto.randomUUID(); accept only that canonical form.
  defp uuid?(value), do: is_binary(value) and match?({:ok, ^value}, Ecto.UUID.cast(value))

  defp valid_room_id?(room_id) do
    match?({:ok, _binary}, Ecto.UUID.dump(room_id))
  end

  defp maybe_put_deck(participant, deck_id) when is_integer(deck_id) do
    case Games.get_deck(deck_id) do
      %{player_id: player_id} = deck when player_id == participant.player_id ->
        Map.merge(participant, %{deck_id: deck.id, deck_name: deck.name})

      _other ->
        participant
    end
  end

  defp maybe_put_deck(participant, _deck_id), do: participant
end
