defmodule TheGatheringWeb.WebcamTableMonarch do
  @moduledoc """
  Ephemeral, single-holder monarch state. Claims and snapshots are serialized so
  all channels see the same holder, including late joiners. The crown is cleared
  when its channel leaves; nothing survives an application restart.
  """
  use GenServer

  alias TheGatheringWeb.Endpoint

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  def sync(topic), do: GenServer.call(__MODULE__, {:sync, topic})

  def take(topic, participant),
    do: GenServer.call(__MODULE__, {:take, topic, participant})

  @impl true
  def init(_opts), do: {:ok, %{}}

  @impl true
  def handle_call({:sync, topic}, {pid, _tag}, rooms) do
    holder =
      case rooms[topic] do
        nil -> nil
        %{holder: holder} -> holder
      end

    send(pid, {:monarch_state, event(holder)})
    {:reply, :ok, rooms}
  end

  def handle_call({:take, topic, participant}, {pid, _tag}, rooms) do
    holder = Map.take(participant, [:peer_id, :player_name])

    case rooms[topic] do
      %{holder: ^holder} ->
        {:reply, :ok, rooms}

      previous ->
        if previous, do: Process.demonitor(previous.ref, [:flush])
        ref = Process.monitor(pid)
        Endpoint.broadcast!(topic, "monarch", event(holder))
        {:reply, :ok, Map.put(rooms, topic, %{holder: holder, ref: ref})}
    end
  end

  @impl true
  def handle_info({:DOWN, ref, :process, _pid, _reason}, rooms) do
    case Enum.find(rooms, fn {_topic, state} -> state.ref == ref end) do
      {topic, _state} ->
        Endpoint.broadcast!(topic, "monarch", event(nil))
        {:noreply, Map.delete(rooms, topic)}

      nil ->
        {:noreply, rooms}
    end
  end

  # Snapshots travel through channels; broadcasts may use fastlane delivery.
  # A revision keeps an older snapshot from overwriting a newer claim in flight.
  defp event(holder),
    do: %{holder: holder, revision: System.unique_integer([:positive, :monotonic])}
end
