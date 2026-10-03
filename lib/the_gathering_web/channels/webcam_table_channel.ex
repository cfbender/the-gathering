defmodule TheGatheringWeb.WebcamTableChannel do
  @moduledoc false

  use TheGatheringWeb, :channel

  require Logger

  alias TheGathering.{Games, WebcamTables}
  alias TheGathering.WebcamTables.Sfu
  alias TheGatheringWeb.{ChannelRateLimit, Presence, WebcamTableRooms}

  @starting_life 40
  @life_range -999..999
  # SDP offers with many candidates run 10–20 KB; anything far larger is abuse.
  @max_sdp_bytes 65_536
  # Direct seat-to-seat messages carry card crops (JPEG data URLs) for the scanner.
  @max_peer_message_bytes 262_144
  # No webcam publishes more rows than 8K; anything above is a bogus status.
  @max_camera_height 4_320
  @signal_events ["sfu_offer", "sfu_answer", "sfu_candidate", "sfu_layer", "peer_message"]

  intercept ["presence_diff"]

  @impl true
  def join("webcam_table:" <> room_id, params, socket) do
    with :ok <- join_rate_limit(socket.assigns.user.id),
         true <- valid_room_id?(room_id),
         {:ok, participant} <- participant(params, socket.assigns.user.id),
         {:ok, state, participant, room_monitor} <- WebcamTables.join(room_id, participant) do
      send(self(), :after_join)
      owner? = room_owner?(state, participant, socket.assigns.user)

      {:ok, %{participant: participant, table_state: state, owner: owner?},
       socket
       |> assign(:participant, participant)
       |> assign(:room_id, room_id)
       |> assign(:room_monitor, room_monitor)
       |> assign(:joined_at, System.monotonic_time(:millisecond))
       |> assign(:rate_limits, %{
         events: ChannelRateLimit.new(:webcam_table_events),
         signals: ChannelRateLimit.new(:webcam_table_signals)
       })
       |> assign(:owner?, owner?)}
    else
      # Only an invalid room id fails `valid_room_id?/1`; a full room is reported by the room.
      false -> {:error, %{reason: "invalid room"}}
      {:error, reason} -> {:error, %{reason: reason}}
    end
  end

  # Why and after how long a seat's channel went away. A seat that keeps rejoining shows up
  # here as short lifetimes; `:shutdown` reasons are the transport closing under it.
  @impl true
  def terminate(reason, socket) do
    case socket.assigns do
      %{participant: %{peer_id: peer_id}, joined_at: joined_at} ->
        lifetime = System.monotonic_time(:millisecond) - joined_at

        Logger.info(
          "Webcam table seat #{peer_id} left after #{lifetime}ms: #{inspect(reason, limit: 50)}"
        )

      _not_joined ->
        :ok
    end

    :ok
  end

  @impl true
  def handle_info(:after_join, socket) do
    participant = socket.assigns.participant
    {:ok, _ref} = Presence.track(socket, participant.peer_id, participant)

    unless participant.spectator,
      do: WebcamTableRooms.track_seat(socket.assigns.room_id, participant)

    state = WebcamTables.snapshot(socket.assigns.room_id)
    push(socket, "table_state", state)
    push(socket, "presence_state", Presence.list(socket))
    push(socket, "monarch_state", state.monarch)

    # Sent once per join rather than in every table_state broadcast; new entries follow as log_entry.
    push(socket, "table_log", %{entries: WebcamTables.log(socket.assigns.room_id)})

    # The seat's media connection lives in the SFU, which monitors this process; the
    # browser offers once the join reply arrives.
    case Sfu.join(socket.assigns.room_id, participant.peer_id, participant.spectator) do
      :ok -> {:noreply, assign(socket, :participant, participant)}
      {:error, reason} -> {:stop, {:sfu_unavailable, reason}, socket}
    end
  end

  # Server-initiated signaling: offers whenever the set of boards changes, trickle ICE,
  # and messages another seat addressed to this one.
  def handle_info({:sfu, :offer, payload}, socket) do
    push(socket, "sfu_offer", payload)
    {:noreply, socket}
  end

  def handle_info({:sfu, :candidate, payload}, socket) do
    push(socket, "sfu_candidate", payload)
    {:noreply, socket}
  end

  def handle_info({:sfu, :peer_message, payload}, socket) do
    push(socket, "peer_message", payload)
    {:noreply, socket}
  end

  # The media connection failed or crashed. Stopping abnormally sends phx_error, so the
  # client rejoins under a new peer id and negotiates a fresh connection.
  def handle_info({:sfu, :down, reason}, socket), do: {:stop, {:sfu_down, reason}, socket}

  def handle_info(:seat_replaced, socket) do
    push(socket, "seat_replaced", %{})
    {:stop, :normal, socket}
  end

  def handle_info({:seat_eliminated, eliminated}, socket) do
    participant = %{socket.assigns.participant | eliminated: eliminated}
    {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
    {:noreply, assign(socket, :participant, participant)}
  end

  # A rematch reset this seat. Adopting the room's copy keeps later status updates from
  # restoring the old game's life and counters; the client rehydrates its local controls.
  def handle_info({:seat_reset, participant}, socket) do
    {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
    push(socket, "seat_reset", %{participant: participant})
    {:noreply, assign(socket, :participant, participant)}
  end

  # The owner ended the table. Stopping normally sends phx_close, so the client
  # leaves instead of rejoining (which would open a fresh room under the same id).
  def handle_info(
        {:DOWN, ref, :process, _pid, {:shutdown, :closed}},
        %{assigns: %{room_monitor: ref}} = socket
      ) do
    push(socket, "table_closed", %{})
    {:stop, :normal, socket}
  end

  # The room crashed. Stopping abnormally sends the client phx_error, so it
  # rejoins a fresh room process restored from the saved session.
  def handle_info(
        {:DOWN, ref, :process, _pid, reason},
        %{assigns: %{room_monitor: ref}} = socket
      ),
      do: {:stop, {:room_down, reason}, socket}

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
  # larger bucket because connecting to a table sends dozens of candidates at once.
  @impl true
  def handle_in(event, payload, socket) do
    bucket = if event in @signal_events, do: :signals, else: :events

    case ChannelRateLimit.take(socket.assigns.rate_limits[bucket]) do
      {:ok, updated} ->
        rate_limits = Map.put(socket.assigns.rate_limits, bucket, updated)
        handle_event(event, payload, assign(socket, :rate_limits, rate_limits))

      {:error, :rate_limited} ->
        {:reply, {:error, %{reason: "rate limited"}}, socket}
    end
  end

  defp handle_event(event, _payload, %{assigns: %{participant: %{spectator: true}}} = socket)
       when event not in ["timer_sync" | @signal_events] do
    {:reply, {:error, %{reason: "spectators cannot change the game"}}, socket}
  end

  defp handle_event(event, _payload, %{assigns: %{owner?: false}} = socket)
       when event in [
              "start_game",
              "seat_order",
              "arrange_seats",
              "set_mode",
              "turn_settings",
              "adjust_turn",
              "timer",
              "end_game",
              "rematch"
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

  # WebRTC signaling with the SFU. The browser's one offer carries its camera; every later
  # offer comes from the server and the browser answers it.
  defp handle_event("sfu_offer", %{"sdp" => sdp} = payload, socket)
       when map_size(payload) == 1 and is_binary(sdp) and byte_size(sdp) <= @max_sdp_bytes do
    case Sfu.offer(socket.assigns.room_id, socket.assigns.participant.peer_id, sdp) do
      {:ok, answer} -> {:reply, {:ok, %{sdp: answer}}, socket}
      {:error, _reason} -> {:reply, {:error, %{reason: "offer rejected"}}, socket}
    end
  end

  defp handle_event("sfu_offer", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid offer"}}, socket}

  defp handle_event("sfu_answer", %{"sdp" => sdp} = payload, socket)
       when map_size(payload) == 1 and is_binary(sdp) and byte_size(sdp) <= @max_sdp_bytes do
    case Sfu.answer(socket.assigns.room_id, socket.assigns.participant.peer_id, sdp) do
      :ok -> {:reply, :ok, socket}
      {:error, _reason} -> {:reply, {:error, %{reason: "answer rejected"}}, socket}
    end
  end

  defp handle_event("sfu_answer", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid answer"}}, socket}

  defp handle_event("sfu_candidate", %{"candidate" => %{"candidate" => _} = candidate}, socket) do
    case Sfu.candidate(socket.assigns.room_id, socket.assigns.participant.peer_id, candidate) do
      :ok -> {:noreply, socket}
      {:error, _reason} -> {:reply, {:error, %{reason: "invalid candidate"}}, socket}
    end
  end

  defp handle_event("sfu_candidate", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid candidate"}}, socket}

  # Which simulcast layer of another seat's board this browser wants, from its drawn size.
  defp handle_event("sfu_layer", %{"peer_id" => owner, "layer" => layer} = payload, socket)
       when map_size(payload) == 2 and is_binary(owner) and is_binary(layer) do
    if uuid?(owner) and Sfu.valid_layer?(layer) do
      case Sfu.layer(socket.assigns.room_id, socket.assigns.participant.peer_id, owner, layer) do
        :ok -> {:noreply, socket}
        {:error, _reason} -> {:reply, {:error, %{reason: "unknown board"}}, socket}
      end
    else
      {:reply, {:error, %{reason: "invalid layer"}}, socket}
    end
  end

  defp handle_event("sfu_layer", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid layer"}}, socket}

  # A message for one other seat (card crops for the scanner), relayed as-is.
  defp handle_event("peer_message", %{"to" => to, "message" => message} = payload, socket)
       when map_size(payload) == 2 and is_binary(to) and is_map(message) do
    cond do
      not uuid?(to) or to == socket.assigns.participant.peer_id ->
        {:reply, {:error, %{reason: "invalid recipient"}}, socket}

      byte_size(Jason.encode!(message)) > @max_peer_message_bytes ->
        {:reply, {:error, %{reason: "message too large"}}, socket}

      true ->
        from = socket.assigns.participant.peer_id

        case Sfu.relay(socket.assigns.room_id, from, to, message) do
          :ok -> {:noreply, socket}
          {:error, _reason} -> {:reply, {:error, %{reason: "recipient has left"}}, socket}
        end
    end
  end

  defp handle_event("peer_message", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid message"}}, socket}

  defp handle_event("choose_deck", %{"deck_id" => deck_id}, socket) when is_integer(deck_id) do
    participant = socket.assigns.participant

    case Games.get_deck(deck_id) do
      %{player_id: player_id} = deck when player_id == participant.player_id ->
        participant = Map.merge(participant, %{deck_id: deck.id, deck_name: deck.name})
        {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
        # Peers may have cached the deck list before this deck was created or edited.
        broadcast!(socket, "deck_selected", %{deck_id: deck.id})
        WebcamTables.remember_seat(socket.assigns.room_id, participant)
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
        WebcamTables.remember_seat(socket.assigns.room_id, participant)

        if Map.has_key?(changes, :eliminated),
          do:
            WebcamTables.eliminate(
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
    participant = socket.assigns.participant
    :ok = WebcamTables.take_monarch(socket.assigns.room_id, participant, participant)
    {:reply, :ok, socket}
  end

  # Any player may hand the monarch to another present, seated player.
  defp handle_event("take_monarch", %{"peer_id" => peer_id} = payload, socket)
       when map_size(payload) == 1 and is_binary(peer_id) do
    seat =
      Enum.find(
        WebcamTables.snapshot(socket.assigns.room_id).seats,
        &(&1.peer_id == peer_id)
      )

    if seat && Map.has_key?(Presence.list(socket), peer_id) do
      :ok = WebcamTables.take_monarch(socket.assigns.room_id, seat, socket.assigns.participant)
      {:reply, :ok, socket}
    else
      {:reply, {:error, %{reason: "the monarch must go to a seated player"}}, socket}
    end
  end

  defp handle_event("take_monarch", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid monarch claim"}}, socket}

  # The room owner may eliminate/restore any present seat; other players only
  # their own. The target channel owns its presence update, so subsequent
  # life/camera updates cannot overwrite it.
  defp handle_event(
         "set_eliminated",
         %{"peer_id" => peer_id, "eliminated" => eliminated} = payload,
         socket
       )
       when map_size(payload) == 2 and is_binary(peer_id) and is_boolean(eliminated) do
    if (socket.assigns.owner? or peer_id == socket.assigns.participant.peer_id) and
         Enum.any?(
           WebcamTables.snapshot(socket.assigns.room_id).seats,
           &(&1.peer_id == peer_id)
         ) and
         Map.has_key?(Presence.list(socket), peer_id) do
      WebcamTables.eliminate(socket.assigns.room_id, peer_id, eliminated)

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
    state = WebcamTables.snapshot(socket.assigns.room_id)

    present =
      state.seats
      |> Enum.map(& &1.peer_id)
      |> Enum.sort()

    if Enum.all?(peer_ids, &is_binary/1) and Enum.sort(peer_ids) == present do
      reply =
        if event == "arrange_seats" or state.mode != "commander",
          do: WebcamTables.arrange(socket.assigns.room_id, peer_ids),
          else: WebcamTables.order(socket.assigns.room_id, peer_ids)

      {:reply, reply, socket}
    else
      {:reply, {:error, %{reason: "seat order must list every seated player"}}, socket}
    end
  end

  defp handle_event(event, _payload, socket) when event in ["seat_order", "arrange_seats"],
    do: {:reply, {:error, %{reason: "invalid seat order"}}, socket}

  defp handle_event("set_mode", %{"mode" => mode} = payload, socket)
       when map_size(payload) == 1 and mode in ["commander", "two_headed_giant", "five_star"] do
    {:reply, WebcamTables.mode(socket.assigns.room_id, mode), socket}
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
    actor = if socket.assigns.owner?, do: :owner, else: socket.assigns.participant.player_id
    {:reply, WebcamTables.adjust_team_life(socket.assigns.room_id, actor, team, delta), socket}
  end

  defp handle_event("adjust_team_life", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid team life adjustment"}}, socket}

  # Closes the table for every seat, whether or not the result was recorded.
  # Each connection, this one included, then receives `table_closed` and stops.
  defp handle_event("end_game", payload, socket) when payload == %{} do
    {:reply, WebcamTables.close(socket.assigns.room_id), socket}
  end

  defp handle_event("end_game", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid end game"}}, socket}

  # Resets the same room to a fresh lobby, whether or not the result was recorded. Every
  # seat receives the new `table_state` and `table_log`; each seated connection also gets
  # `seat_reset` with its reset seat. Nobody leaves, so the table stays open.
  defp handle_event("rematch", payload, socket) when payload == %{} do
    {:reply, WebcamTables.rematch(socket.assigns.room_id), socket}
  end

  defp handle_event("rematch", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid rematch"}}, socket}

  # An explicit `randomize` overrides the room's auto-randomize setting for this start.
  defp handle_event("start_game", payload, socket) when payload == %{} do
    {:reply, WebcamTables.start_game(socket.assigns.room_id), socket}
  end

  defp handle_event("start_game", %{"randomize" => randomize} = payload, socket)
       when map_size(payload) == 1 and is_boolean(randomize) do
    {:reply, WebcamTables.start_game(socket.assigns.room_id, randomize), socket}
  end

  defp handle_event("start_game", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid start"}}, socket}

  defp handle_event("turn_settings", %{"auto_randomize" => enabled} = payload, socket)
       when map_size(payload) == 1 and is_boolean(enabled) do
    {:reply, WebcamTables.turn_settings(socket.assigns.room_id, enabled), socket}
  end

  defp handle_event("turn_settings", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid turn settings"}}, socket}

  defp handle_event("pass_turn", %{"revision" => revision} = payload, socket)
       when map_size(payload) == 1 and is_integer(revision) and revision >= 0 do
    {:reply, WebcamTables.pass_turn(socket.assigns.room_id, revision), socket}
  end

  defp handle_event("pass_turn", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid pass turn"}}, socket}

  defp handle_event("unpass_turn", %{"revision" => revision} = payload, socket)
       when map_size(payload) == 1 and is_integer(revision) and revision >= 0 do
    {:reply, WebcamTables.unpass_turn(socket.assigns.room_id, revision), socket}
  end

  defp handle_event("unpass_turn", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid un-pass turn"}}, socket}

  defp handle_event(
         "adjust_turn",
         %{"player_id" => player_id, "delta" => delta} = payload,
         socket
       )
       when map_size(payload) == 2 and is_integer(player_id) and delta in [-1, 1] do
    {:reply, WebcamTables.adjust_turn(socket.assigns.room_id, player_id, delta), socket}
  end

  defp handle_event("adjust_turn", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid turn adjustment"}}, socket}

  defp handle_event("timer", %{"action" => action} = payload, socket)
       when map_size(payload) == 1 and action in ["pause", "resume"] do
    timer = WebcamTables.timer(socket.assigns.room_id, action)
    {:reply, {:ok, timer}, socket}
  end

  defp handle_event("timer", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid timer action"}}, socket}

  defp handle_event("begin_play", payload, socket) when payload == %{} do
    actor = if socket.assigns.owner?, do: :owner, else: socket.assigns.participant.player_id
    {:reply, WebcamTables.begin_play(socket.assigns.room_id, actor), socket}
  end

  defp handle_event("begin_play", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid start"}}, socket}

  defp handle_event("timer_sync", payload, socket) when payload == %{} do
    {:reply, {:ok, WebcamTables.snapshot(socket.assigns.room_id).timer}, socket}
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
    do: WebcamTables.cards(socket.assigns.room_id, payload, socket.assigns.participant)

  # Presence tells every seat who may see the board; the SFU enforces it on the media.
  defp put_reveal(socket, target) do
    participant = %{socket.assigns.participant | reveal_to: target}
    {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
    WebcamTables.remember_seat(socket.assigns.room_id, participant)
    _ = Sfu.reveal(socket.assigns.room_id, participant.peer_id, target)
    assign(socket, :participant, participant)
  end

  defp broadcast_roll(socket, roll),
    do: WebcamTables.roll(socket.assigns.room_id, socket.assigns.participant, roll)

  defp status_changes(payload) do
    Enum.reduce_while(payload, {:ok, %{}}, fn
      {"life", life}, {:ok, changes} when is_integer(life) and life in @life_range ->
        {:cont, {:ok, Map.put(changes, :life, life)}}

      {"camera_off", camera_off}, {:ok, changes} when is_boolean(camera_off) ->
        {:cont, {:ok, Map.put(changes, :camera_off, camera_off)}}

      # The camera's native rows, so a viewer receiving the full layer crops its own frame
      # instead of asking the owner; nil while only the placeholder is published.
      {"camera_height", height}, {:ok, changes}
      when is_nil(height) or (is_integer(height) and height in 1..@max_camera_height) ->
        {:cont, {:ok, Map.put(changes, :camera_height, height)}}

      # Whether crops of this board may be uploaded as recognizer training data.
      {"shares_corrections", shares}, {:ok, changes} when is_boolean(shares) ->
        {:cont, {:ok, Map.put(changes, :shares_corrections, shares)}}

      {key, count}, {:ok, changes}
      when key in ["poison", "rad"] and is_integer(count) and count in 0..999 ->
        field = if key == "poison", do: :poison, else: :rad
        {:cont, {:ok, Map.put(changes, field, count)}}

      {"commander_casts", counts}, {:ok, changes} ->
        put_valid(changes, :commander_casts, counts, &valid_counts?/1)

      {"commander_damage", damage}, {:ok, changes} ->
        put_valid(changes, :commander_damage, damage, &valid_damage?/1)

      {"eliminated", eliminated}, {:ok, changes} when is_boolean(eliminated) ->
        {:cont, {:ok, Map.put(changes, :eliminated, eliminated)}}

      {"custom_counters", counters}, {:ok, changes} ->
        put_valid(changes, :custom_counters, counters, &valid_custom_counters?/1)

      {"combat_effects", effects}, {:ok, changes} ->
        put_valid(changes, :combat_effects, effects, &valid_combat_effects?/1)

      _invalid, _changes ->
        {:halt, :error}
    end)
  end

  defp put_valid(changes, field, value, valid?) do
    if valid?.(value), do: {:cont, {:ok, Map.put(changes, field, value)}}, else: {:halt, :error}
  end

  # Free-form counters a seat shares ("Lands: 7"). The client keeps private ones to itself.
  defp valid_custom_counters?(counters) when is_list(counters) and length(counters) <= 20 do
    Enum.all?(counters, fn
      %{"id" => id, "label" => label, "value" => value} = counter when map_size(counter) == 3 ->
        short_string?(id, 40) and short_string?(label, 40) and is_integer(value) and
          value in 0..100

      _other ->
        false
    end)
  end

  defp valid_custom_counters?(_counters), do: false

  # Anthems and combat buffs a seat shares; every client derives the same totals from them.
  defp valid_combat_effects?(effects) when is_list(effects) and length(effects) <= 30 do
    Enum.all?(effects, fn
      %{
        "id" => id,
        "name" => name,
        "power" => power,
        "toughness" => toughness,
        "conditions" => conditions,
        "keywords" => keywords
      } = effect
      when map_size(effect) == 6 ->
        short_string?(id, 40) and is_binary(name) and byte_size(name) <= 80 and
          buff_amount?(power) and buff_amount?(toughness) and
          string_list?(conditions, 8) and string_list?(keywords, 10)

      _other ->
        false
    end)
  end

  defp valid_combat_effects?(_effects), do: false

  defp buff_amount?(value), do: is_integer(value) and value in -99..99

  defp short_string?(value, max), do: is_binary(value) and byte_size(value) in 1..max

  defp string_list?(items, max_items) when is_list(items) and length(items) <= max_items,
    do: Enum.all?(items, &short_string?(&1, 40))

  defp string_list?(_items, _max_items), do: false

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
          camera_height: nil,
          shares_corrections: false,
          poison: 0,
          rad: 0,
          commander_casts: %{},
          commander_damage: %{},
          custom_counters: [],
          combat_effects: [],
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

  # Admins run every table they sit at, alongside the player who opened it.
  # Spectators never hold table controls.
  defp room_owner?(_state, %{spectator: true}, _user), do: false
  defp room_owner?(_state, _participant, %{role: "admin"}), do: true
  defp room_owner?(state, participant, _user), do: state.owner_id == participant.player_id

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
