defmodule TheGathering.Imports do
  @moduledoc """
  Imports game history from external data sources.

  Every source parses into normalized `Imports.Game` and `Imports.Seat` structs.
  `preview/2` matches players and decks against existing records, while `import/3` commits the whole
  batch in one transaction. Games are keyed by `{source, external_id}`, so
  re-importing the same data skips games that already exist.
  """

  alias TheGathering.Imports.{Commit, Preview}

  @sources %{csv: "csv", mythic_track: "mythic_track"}

  def preview_csv(csv), do: preview(:csv, csv)
  def import_csv(csv, user_id), do: import(:csv, csv, user_id)

  def preview(source, payload) when is_map_key(@sources, source) and is_binary(payload),
    do: Preview.run(source, payload)

  def import(source, payload, user_id) when is_map_key(@sources, source) and is_binary(payload) do
    Commit.run(source, payload, @sources[source], user_id)
  end
end
