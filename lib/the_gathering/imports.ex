defmodule TheGathering.Imports do
  @moduledoc """
  Imports game history from external data sources.

  CSV and Mythic Track parse into normalized `Imports.Game` and `Imports.Seat` structs.
  `preview/2` matches players and decks against existing records, while `import/3` commits the whole
  batch in one transaction. Games are keyed by `{source, external_id}`, so
  re-importing the same data skips games that already exist.

  `preview_sheet/1` and `import_sheet/3` reconcile the original Google Sheet using
  explicit mappings and update/create/skip choices, preserving existing game identities.
  """

  alias TheGathering.Imports.{
    Commit,
    PortableExport,
    PortableImport,
    Preview,
    SheetCommit,
    SheetPreview
  }

  @sources %{csv: "csv", mythic_track: "mythic_track"}

  def export_portable, do: PortableExport.run()
  def preview_portable(json), do: PortableImport.preview(json)
  def import_portable(json, user_id), do: PortableImport.run(json, user_id)

  def preview_sheet(params), do: SheetPreview.run(params)
  def import_sheet(params, revision, user_id), do: SheetCommit.run(params, revision, user_id)

  def preview_csv(csv), do: preview(:csv, csv)
  def import_csv(csv, user_id), do: import(:csv, csv, user_id)

  def preview(source, payload) when is_map_key(@sources, source) and is_binary(payload),
    do: Preview.run(source, payload)

  def import(source, payload, user_id) when is_map_key(@sources, source) and is_binary(payload) do
    Commit.run(source, payload, @sources[source], user_id)
  end
end
