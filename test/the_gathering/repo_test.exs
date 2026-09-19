defmodule TheGathering.RepoTest do
  use ExUnit.Case, async: true

  alias TheGathering.Repo

  # Deferred SQLite transactions that read before writing fail immediately with
  # "Database busy" when another connection commits in between, e.g. registering
  # the first admin while the catalog sync is running. Immediate mode queues on
  # busy_timeout instead.
  test "write transactions take the SQLite write lock up front" do
    config = Repo.config()

    assert config[:default_transaction_mode] == :immediate
    assert config[:busy_timeout] >= 5_000
  end
end
