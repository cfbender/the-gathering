defmodule TheGathering.Decklists.Cache do
  @moduledoc "In-memory cache for successful deck resolutions. Entries expire after five minutes."

  use GenServer

  @table __MODULE__
  @ttl_ms :timer.minutes(5)

  def start_link(_options), do: GenServer.start_link(__MODULE__, :ok, name: __MODULE__)

  def fetch(key) do
    now = System.monotonic_time(:millisecond)

    case :ets.lookup(@table, key) do
      [{^key, value, expires_at}] when expires_at > now -> {:ok, value}
      [{^key, _value, _expires_at}] -> delete(key)
      [] -> :miss
    end
  end

  def put(key, value) do
    expires_at = System.monotonic_time(:millisecond) + ttl_ms()
    true = :ets.insert(@table, {key, value, expires_at})
    :ok
  end

  def clear, do: :ets.delete_all_objects(@table)

  @impl true
  def init(:ok) do
    :ets.new(@table, [:named_table, :set, :public, read_concurrency: true])
    {:ok, nil}
  end

  defp delete(key) do
    :ets.delete(@table, key)
    :miss
  end

  defp ttl_ms do
    Application.get_env(:the_gathering, :decklists_cache_ttl_ms, @ttl_ms)
  end
end
