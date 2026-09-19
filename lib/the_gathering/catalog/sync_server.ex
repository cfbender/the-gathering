defmodule TheGathering.Catalog.SyncServer do
  @moduledoc false

  use GenServer

  alias TheGathering.Catalog
  alias TheGathering.Catalog.Sync

  @week_ms 7 * 24 * 60 * 60 * 1_000

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  def trigger, do: GenServer.call(__MODULE__, :trigger)

  @impl true
  def init(_opts) do
    interval = Application.get_env(:the_gathering, :catalog_sync_interval_ms, @week_ms)
    enabled? = Application.get_env(:the_gathering, :catalog_sync_enabled, true)
    timer = if enabled?, do: Process.send_after(self(), :scheduled_sync, initial_delay(interval))

    {:ok, %{task: nil, timer: timer, interval: interval, enabled?: enabled?}}
  end

  @impl true
  def handle_call(:trigger, _from, %{task: nil} = state) do
    {:reply, :started, state |> cancel_timer() |> start_sync()}
  end

  def handle_call(:trigger, _from, state), do: {:reply, :already_running, state}

  @impl true
  def handle_info(:scheduled_sync, %{task: nil} = state) do
    {:noreply, state |> Map.put(:timer, nil) |> start_sync()}
  end

  def handle_info(:scheduled_sync, state), do: {:noreply, state}

  def handle_info({reference, _result}, %{task: %{ref: reference}} = state) do
    Process.demonitor(reference, [:flush])
    {:noreply, state |> Map.put(:task, nil) |> schedule()}
  end

  def handle_info({:DOWN, reference, :process, _pid, _reason}, %{task: %{ref: reference}} = state) do
    {:noreply, state |> Map.put(:task, nil) |> schedule()}
  end

  def handle_info(_message, state), do: {:noreply, state}

  defp start_sync(state) do
    task = Task.Supervisor.async_nolink(TheGathering.Catalog.TaskSupervisor, Sync, :run, [])
    %{state | task: task}
  end

  defp schedule(%{enabled?: true} = state) do
    timer = Process.send_after(self(), :scheduled_sync, state.interval)
    %{state | timer: timer}
  end

  defp schedule(state), do: state

  defp cancel_timer(%{timer: nil} = state), do: state

  defp cancel_timer(state) do
    Process.cancel_timer(state.timer)
    %{state | timer: nil}
  end

  defp initial_delay(interval) do
    if Catalog.count_cards() == 0, do: 1_000, else: interval
  end
end
