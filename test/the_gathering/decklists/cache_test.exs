defmodule TheGathering.Decklists.CacheTest do
  use ExUnit.Case, async: false

  alias TheGathering.Accounts.User
  alias TheGathering.Decklists.{Cache, RemoteDecks}

  setup do
    Cache.clear()

    on_exit(fn ->
      Application.delete_env(:the_gathering, :decklists_cache_ttl_ms)
      Application.delete_env(:the_gathering, :decklists_cache_max_entries)
      Cache.clear()
    end)
  end

  test "sweep removes expired entries that are never fetched again" do
    Application.put_env(:the_gathering, :decklists_cache_ttl_ms, 0)
    :ok = Cache.put(:expired, :value)
    :ok = Cache.sweep()
    assert :ets.lookup(Cache, :expired) == []
  end

  test "size cap evicts an old entry" do
    Application.put_env(:the_gathering, :decklists_cache_max_entries, 1)
    :ok = Cache.put(:first, :value)
    :ok = Cache.put(:second, :value)
    assert :ets.info(Cache, :size) == 1
    assert {:ok, :value} = Cache.fetch(:second)
  end

  test "remote cache keys and values do not contain plaintext API keys" do
    secret = "sentinel-plain-api-key"

    RemoteDecks.list(%User{
      id: 42,
      display_name: "User",
      manavault_api_key: secret
    })

    cache_contents = Cache |> :ets.tab2list() |> :erlang.term_to_binary()
    refute cache_contents =~ secret
    assert [{:remote_decks, 42}] = :ets.select(Cache, [{{:"$1", :_, :_, :_}, [], [:"$1"]}])
  end
end
