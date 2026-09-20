defmodule TheGathering.Catalog.Backfill do
  @moduledoc "Runs bounded historical repair for Games-owned catalog references."

  alias TheGathering.Games.LinkCatalogCards

  @type summary :: %{
          decks_split: non_neg_integer(),
          decks_linked: non_neg_integer(),
          colors_filled: non_neg_integer(),
          mvps_linked: non_neg_integer(),
          unmatched: [String.t()]
        }

  @spec run() :: summary()
  def run, do: repair(%{deck_id: 0, seat_id: 0}, LinkCatalogCards.empty_summary())

  @doc "Compatibility wrapper for callers parsing Mythic Track partner notation."
  def split_partners(name), do: LinkCatalogCards.split_partners(name)

  @doc "Compatibility wrapper; new callers should use `TheGathering.Catalog.find_card_by_name/1`."
  def find_by_name(name), do: TheGathering.Catalog.find_card_by_name(name)

  defp repair(cursor, summary) do
    {:ok, result} = LinkCatalogCards.repair_batch(cursor)
    summary = LinkCatalogCards.merge_summaries(summary, result.summary)

    if result.done?, do: summary, else: repair(result.cursor, summary)
  end
end
