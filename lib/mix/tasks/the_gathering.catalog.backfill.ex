defmodule Mix.Tasks.TheGathering.Catalog.Backfill do
  @moduledoc """
  Links imported decks and MVP cards to catalog cards by name.

  Splits Mythic Track "Commander || Partner" names, fills Scryfall IDs and
  missing color identities, and reports names that no catalog card matched.
  Safe to rerun; only missing data is filled.
  """

  use Mix.Task

  alias TheGathering.Catalog.Backfill

  @shortdoc "Links imported decks and MVP cards to the card catalog"

  @impl true
  def run(_args) do
    Application.put_env(:the_gathering, :catalog_sync_enabled, false)
    Mix.Task.run("app.start")

    summary = Backfill.run()

    Mix.shell().info(
      "Split #{summary.decks_split} partner decks, linked #{summary.decks_linked} commanders, " <>
        "filled #{summary.colors_filled} color identities, linked #{summary.mvps_linked} MVP cards"
    )

    Enum.each(summary.unmatched, &Mix.shell().info("  unmatched: #{&1}"))
  end
end
