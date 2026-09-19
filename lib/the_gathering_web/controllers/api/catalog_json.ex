defmodule TheGatheringWeb.API.CatalogJSON do
  alias TheGathering.Catalog.SyncState

  def show(%{sync: sync}), do: %{data: sync(sync)}

  def triggered(%{result: result}) do
    %{data: %{status: if(result == :started, do: "started", else: "already_running")}}
  end

  defp sync(%SyncState{} = state) do
    %{
      status: state.status,
      last_started_at: state.last_started_at,
      last_finished_at: state.last_finished_at,
      card_count: state.card_count,
      scryfall_updated_at: state.scryfall_updated_at,
      last_error: state.last_error
    }
  end
end
