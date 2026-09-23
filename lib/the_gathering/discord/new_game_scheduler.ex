defmodule TheGathering.Discord.NewGameScheduler do
  @moduledoc "Single writer for queue interactions and Discord edits; timers contain no game state."
  use GenServer
  alias TheGathering.Discord.{NewGameDelivery, ScheduledGames}

  def start_link(opts),
    do: GenServer.start_link(__MODULE__, opts, name: Keyword.get(opts, :name, __MODULE__))

  def act(id, action, actor, server \\ __MODULE__),
    do: GenServer.call(server, {:act, id, action, actor}, :infinity)

  def attach(id, message_id, server \\ __MODULE__),
    do: GenServer.call(server, {:attach, id, message_id}, :infinity)

  def sweep(server \\ __MODULE__), do: GenServer.call(server, :sweep, :infinity)

  @impl true
  def init(opts) do
    state = %{
      api: Keyword.get(opts, :api, Nostrum.Api.Message),
      now: Keyword.get(opts, :now, &DateTime.utc_now/0),
      interval: Keyword.get(opts, :interval, 5000)
    }

    # Reload pending work immediately on every boot, not only after the first interval.
    send(self(), :tick)
    {:ok, state}
  end

  @impl true
  def handle_call({:act, id, action, actor}, _from, state) do
    result = ScheduledGames.act(id, action, actor, state.now.())

    case result do
      {:ok, game} -> NewGameDelivery.deliver(game, state.api)
      _ -> :ok
    end

    {:reply, result, state}
  end

  def handle_call({:attach, id, message_id}, _from, state) do
    ScheduledGames.attach_message(id, message_id)
    {:ok, game} = ScheduledGames.settle(id, state.now.())
    NewGameDelivery.deliver(game, state.api)
    {:reply, :ok, state}
  end

  def handle_call(:sweep, _from, state) do
    run(state)
    {:reply, :ok, state}
  end

  @impl true
  def handle_info(:tick, state) do
    run(state)
    Process.send_after(self(), :tick, state.interval)
    {:noreply, state}
  end

  defp run(state) do
    run_batch(state, state.now.(), 0)
  end

  defp run_batch(state, now, after_id) do
    ids = ScheduledGames.pending_ids(now, after_id)

    for id <- ids do
      {:ok, game} = ScheduledGames.settle(id, now)
      NewGameDelivery.deliver(game, state.api)
    end

    if length(ids) == 100, do: run_batch(state, now, List.last(ids))
  end
end
