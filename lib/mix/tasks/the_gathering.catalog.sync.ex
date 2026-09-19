defmodule Mix.Tasks.TheGathering.Catalog.Sync do
  @moduledoc "Synchronizes the local card catalog from Scryfall bulk data."

  use Mix.Task

  alias TheGathering.Catalog.Sync, as: CatalogSync

  @shortdoc "Synchronizes the local card catalog from Scryfall"

  @impl true
  def run(_args) do
    # This invocation owns the sync; do not also enqueue the automatic boot sync.
    Application.put_env(:the_gathering, :catalog_sync_enabled, false)
    Mix.Task.run("app.start")

    case CatalogSync.run() do
      {:ok, count} -> Mix.shell().info("Catalog synchronized: #{count} cards")
      {:error, reason} -> Mix.raise("Catalog sync failed: #{reason}")
    end
  end
end
