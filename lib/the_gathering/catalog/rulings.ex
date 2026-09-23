defmodule TheGathering.Catalog.Rulings do
  @moduledoc false
  alias TheGathering.Catalog.{RulingCache, Scryfall}
  alias TheGathering.Repo

  @ttl_seconds 86_400

  def get(id) do
    now = DateTime.utc_now(:second)

    case Repo.get(RulingCache, id) do
      %RulingCache{fetched_at: fetched_at, rulings: rulings} ->
        if DateTime.diff(now, fetched_at) < @ttl_seconds,
          do: {:ok, rulings},
          else: fetch(id, now)

      nil ->
        fetch(id, now)
    end
  end

  defp fetch(id, now) do
    with {:ok, rulings} <- Scryfall.rulings(id) do
      row = %{id: id, rulings: rulings, fetched_at: now}
      Repo.insert_all(RulingCache, [row], on_conflict: :replace_all, conflict_target: :id)
      {:ok, rulings}
    end
  end
end
