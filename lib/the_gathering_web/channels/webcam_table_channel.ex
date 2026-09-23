defmodule TheGatheringWeb.WebcamTableChannel do
  @moduledoc false

  use TheGatheringWeb, :channel

  alias TheGathering.Games
  alias TheGatheringWeb.{Presence, WebcamTableMonarch, WebcamTableRooms}

  @max_players 4
  @starting_life 40
  @life_range -999..999

  @impl true
  def join("webcam_table:" <> room_id, params, socket) do
    with true <- valid_room_id?(room_id),
         {:ok, participant} <- participant(params, socket.assigns.user.id),
         true <- room_available?(Presence.list(socket), participant) do
      send(self(), :after_join)
      {:ok, socket |> assign(:participant, participant) |> assign(:room_id, room_id)}
    else
      false -> {:error, %{reason: "room is full or invalid"}}
      {:error, reason} -> {:error, %{reason: reason}}
    end
  end

  @impl true
  def handle_info(:after_join, socket) do
    participant = socket.assigns.participant
    {:ok, _ref} = Presence.track(socket, participant.peer_id, participant)
    {:ok, _ref} = WebcamTableRooms.track_seat(socket.assigns.room_id, participant)
    push(socket, "presence_state", Presence.list(socket))
    :ok = WebcamTableMonarch.sync(socket.topic)
    {:noreply, socket}
  end

  def handle_info({:monarch_state, event}, socket) do
    push(socket, "monarch_state", event)
    {:noreply, socket}
  end

  @impl true
  def handle_in("signal", %{"target" => target, "signal" => signal}, socket)
      when is_binary(target) and is_map(signal) do
    broadcast_from!(socket, "signal", %{
      target: target,
      from: socket.assigns.participant.peer_id,
      signal: signal
    })

    {:noreply, socket}
  end

  def handle_in("signal", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid signal"}}, socket}

  def handle_in("choose_deck", %{"deck_id" => deck_id}, socket) when is_integer(deck_id) do
    participant = socket.assigns.participant

    case Games.get_deck(deck_id) do
      %{player_id: player_id} = deck when player_id == participant.player_id ->
        participant = Map.merge(participant, %{deck_id: deck.id, deck_name: deck.name})
        {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
        {:reply, :ok, assign(socket, :participant, participant)}

      _other ->
        {:reply, {:error, %{reason: "deck does not belong to player"}}, socket}
    end
  end

  def handle_in("choose_deck", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid deck"}}, socket}

  # Ephemeral state a player publishes about their own seat, carried by presence.
  def handle_in("update_status", payload, socket) when is_map(payload) do
    case status_changes(payload) do
      {:ok, changes} ->
        participant = Map.merge(socket.assigns.participant, changes)
        {:ok, _ref} = Presence.update(socket, participant.peer_id, participant)
        {:reply, :ok, assign(socket, :participant, participant)}

      :error ->
        {:reply, {:error, %{reason: "invalid status"}}, socket}
    end
  end

  def handle_in("update_status", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid status"}}, socket}

  def handle_in("take_monarch", payload, socket) when payload == %{} do
    :ok = WebcamTableMonarch.take(socket.topic, socket.assigns.participant)
    {:reply, :ok, socket}
  end

  def handle_in("take_monarch", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid monarch claim"}}, socket}

  # Seat order is shared so every browser records the same turn order. The
  # proposed order must name exactly the peers present at that moment.
  def handle_in("seat_order", %{"peer_ids" => peer_ids}, socket) when is_list(peer_ids) do
    present = socket |> Presence.list() |> Map.keys() |> Enum.sort()

    if Enum.all?(peer_ids, &is_binary/1) and Enum.sort(peer_ids) == present do
      broadcast!(socket, "seat_order", %{peer_ids: peer_ids})
      {:reply, :ok, socket}
    else
      {:reply, {:error, %{reason: "seat order must list every seated player"}}, socket}
    end
  end

  def handle_in("seat_order", _payload, socket),
    do: {:reply, {:error, %{reason: "invalid seat order"}}, socket}

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

      _invalid, _changes ->
        {:halt, :error}
    end)
  end

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
       when is_binary(peer_id) and is_integer(player_id) do
    case Games.get_player(player_id) do
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
          # Default seat order is join order, so every browser sees the same seats.
          joined_at: System.system_time(:millisecond)
        }

        {:ok, maybe_put_deck(participant, Map.get(params, "deck_id"))}

      _unlinked_or_other_player ->
        {:error, "account is not linked to this player"}
    end
  end

  defp participant(_params, _user_id), do: {:error, "account is not linked to a player"}

  defp valid_room_id?(room_id) do
    match?({:ok, _binary}, Ecto.UUID.dump(room_id))
  end

  defp room_available?(presences, participant) do
    map_size(presences) < @max_players and
      Enum.all?(presences, fn {_peer_id, %{metas: metas}} ->
        Enum.all?(metas, &(&1.player_id != participant.player_id))
      end)
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
