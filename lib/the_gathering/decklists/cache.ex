defmodule TheGathering.Decklists.Cache do
  @moduledoc "Bounded in-memory cache for successful deck resolutions."

  use GenServer

  @table __MODULE__
  @ttl_ms :timer.minutes(5)
  @sweep_interval_ms :timer.minutes(1)
  @max_entries 1_000

  def start_link(_options), do: GenServer.start_link(__MODULE__, :ok, name: __MODULE__)

  def fetch(key) do
    now = System.monotonic_time(:millisecond)

    case :ets.lookup(@table, key) do
      [{^key, value, expires_at, _inserted_at}] when expires_at > now -> {:ok, value}
      [{^key, _value, _expires_at, _inserted_at}] -> delete(key)
      [] -> :miss
    end
  end

  def put(key, value) do
    GenServer.call(__MODULE__, {:put, key, value})
  end

  def clear, do: :ets.delete_all_objects(@table)
  def sweep, do: GenServer.call(__MODULE__, :sweep)

  @impl true
  def init(:ok) do
    :ets.new(@table, [:named_table, :set, :public, read_concurrency: true])
    schedule_sweep()
    {:ok, nil}
  end

  @impl true
  def handle_call({:put, key, value}, _from, state) do
    now = System.monotonic_time(:millisecond)
    sweep_expired(now)
    make_room(key)
    true = :ets.insert(@table, {key, value, now + ttl_ms(), now})
    {:reply, :ok, state}
  end

  def handle_call(:sweep, _from, state) do
    sweep_expired(System.monotonic_time(:millisecond))
    {:reply, :ok, state}
  end

  @impl true
  def handle_info(:sweep, state) do
    sweep_expired(System.monotonic_time(:millisecond))
    schedule_sweep()
    {:noreply, state}
  end

  defp delete(key) do
    :ets.delete(@table, key)
    :miss
  end

  defp ttl_ms do
    Application.get_env(:the_gathering, :decklists_cache_ttl_ms, @ttl_ms)
  end

  defp sweep_expired(now) do
    :ets.select_delete(@table, [{{:_, :_, :"$1", :_}, [{:"=<", :"$1", now}], [true]}])
  end

  defp make_room(key) do
    if :ets.lookup(@table, key) == [] and :ets.info(@table, :size) >= max_entries() do
      evict_oldest()
    end
  end

  defp evict_oldest do
    @table
    |> :ets.tab2list()
    |> Enum.min_by(&elem(&1, 3), fn -> nil end)
    |> delete_entry()
  end

  defp delete_entry({key, _value, _expires_at, _inserted_at}), do: :ets.delete(@table, key)
  defp delete_entry(nil), do: :ok

  defp max_entries do
    Application.get_env(:the_gathering, :decklists_cache_max_entries, @max_entries)
  end

  defp schedule_sweep do
    Process.send_after(self(), :sweep, @sweep_interval_ms)
  end
end
