defmodule TheGatheringWeb.WebcamTableChannel do
  @moduledoc false

  use TheGatheringWeb, :channel

  alias TheGathering.Games
  alias TheGatheringWeb.Presence

  @max_players 4

  @impl true
  def join("webcam_table:" <> room_id, params, socket) do
    with true <- valid_room_id?(room_id),
         {:ok, participant} <- participant(params, socket.assigns.user.id),
         true <- room_available?(Presence.list(socket), participant) do
      send(self(), :after_join)
      {:ok, assign(socket, :participant, participant)}
    else
      false -> {:error, %{reason: "room is full or invalid"}}
      {:error, reason} -> {:error, %{reason: reason}}
    end
  end

  @impl true
  def handle_info(:after_join, socket) do
    participant = socket.assigns.participant
    {:ok, _ref} = Presence.track(socket, participant.peer_id, participant)
    push(socket, "presence_state", Presence.list(socket))
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

  defp participant(%{"peer_id" => peer_id, "player_id" => player_id} = params, user_id)
       when is_binary(peer_id) and is_integer(player_id) do
    case Games.get_player(player_id) do
      %{user_id: ^user_id} = player ->
        participant = %{peer_id: peer_id, player_id: player.id, player_name: player.name}
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
