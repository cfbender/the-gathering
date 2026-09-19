defmodule TheGathering.CatalogTest do
  use TheGathering.DataCase

  alias TheGathering.Catalog
  alias TheGathering.Catalog.CardData
  alias TheGathering.Catalog.Sync

  @fixture Path.expand("../support/fixtures/scryfall_catalog.jsonl", __DIR__)

  setup do
    assert {:ok, 2} = Sync.run(source: {:file, @fixture})
    :ok
  end

  test "ranks exact names before prefixes and substrings" do
    insert_card("exact", "oracle-exact", "Bolt")
    insert_card("prefix", "oracle-prefix", "Bolt Hound")
    insert_card("substring", "oracle-substring", "Thunder Bolt Adept")

    assert Enum.map(Catalog.search("bolt"), & &1.id) == [
             "exact",
             "prefix",
             "printing-latest",
             "substring"
           ]
  end

  test "folds accents in names" do
    assert [%{name: "Jötun Grunt"}] = Catalog.search("Jotun")
  end

  defp insert_card(id, oracle_id, name) do
    attrs =
      %{
        "id" => id,
        "oracle_id" => oracle_id,
        "name" => name,
        "lang" => "en",
        "games" => ["paper"],
        "released_at" => "2024-01-01",
        "set" => "tst",
        "collector_number" => id,
        "type_line" => "Instant",
        "layout" => "normal",
        "rarity" => "common",
        "legalities" => %{"commander" => "legal"}
      }
      |> CardData.from_scryfall()
      |> Map.delete(:selection_key)

    Repo.insert_all(TheGathering.Catalog.Card, [attrs])
  end
end
