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

  test "matches omitted, straight, and curly apostrophes while retaining ranking" do
    insert_card("jeska-exact", "oracle-jeska-exact", "Jeska's Will")
    insert_card("jeska-prefix", "oracle-jeska-prefix", "Jeskas Willpower")
    insert_card("jeska-substring", "oracle-jeska-substring", "Copy of Jeska’s Will")

    expected = ["jeska-exact", "jeska-prefix", "jeska-substring"]

    assert Enum.map(Catalog.search("Jeskas Will"), & &1.id) == expected
    assert Enum.map(Catalog.search("Jeska's Will"), & &1.id) == expected
    assert Enum.map(Catalog.search("Jeska’s Will"), & &1.id) == expected
  end

  test "matches partial unique names" do
    insert_card("lumra", "oracle-lumra", "Lumra, Bellow of the Woods")

    assert [%{id: "lumra", name: "Lumra, Bellow of the Woods"}] = Catalog.search("lumra")
  end

  test "ignores commas and ranks whole leading names ahead of asymmetric decoys" do
    insert_card("bello", "oracle-bello", "Bello, Bard of the Brambles")
    insert_card("bellowing", "oracle-bellowing", "Bellowing Crier")
    insert_card("lumra", "oracle-lumra", "Lumra, Bellow of the Woods")

    insert_card(
      "sephiroth",
      "oracle-sephiroth",
      "Sephiroth, Fabled SOLDIER // Sephiroth, One-Winged Angel"
    )

    insert_card("terra", "oracle-terra", "Terra, Magical Adept // Esper Terra")

    assert Enum.map(Catalog.search("Bello"), & &1.id) == ["bello", "bellowing", "lumra"]
    assert Enum.map(Catalog.search("sephiroth fabled soldier"), & &1.id) == ["sephiroth"]
    assert Enum.map(Catalog.search("Terra magical adept"), & &1.id) == ["terra"]
  end

  test "comma-insensitive search preserves filters and limits" do
    insert_card("commander", "oracle-commander", "Bello, Bard of the Brambles", %{
      "type_line" => "Legendary Creature — Raccoon Bard"
    })

    for number <- 1..25 do
      insert_card("decoy-#{number}", "oracle-decoy-#{number}", "Bellowing Decoy #{number}")
    end

    assert Enum.map(Catalog.search("Bello", commander: true, limit: 1), & &1.id) == [
             "commander"
           ]

    assert length(Catalog.search("Bello", limit: 50)) == 26
  end

  test "applies commander and partner filters to apostrophe-insensitive matches" do
    insert_card("spell", "oracle-spell", "Hero's Aid")

    insert_card("partner", "oracle-partner", "Heros Aid Captain", %{
      "type_line" => "Legendary Creature — Human",
      "oracle_text" => "Partner"
    })

    assert Enum.map(Catalog.search("heros aid", commander: true), & &1.id) == ["partner"]
    assert Enum.map(Catalog.search("hero’s aid", partner: true), & &1.id) == ["partner"]
  end

  test "treats SQL wildcard characters literally" do
    insert_card("percent", "oracle-percent", "A 100% Real Card")
    insert_card("percent-decoy", "oracle-percent-decoy", "A 100X Real Card")
    insert_card("underscore", "oracle-underscore", "Under_score")
    insert_card("underscore-decoy", "oracle-underscore-decoy", "UnderXscore")

    assert Enum.map(Catalog.search("100%"), & &1.id) == ["percent"]
    assert Enum.map(Catalog.search("under_score"), & &1.id) == ["underscore"]
  end

  defp insert_card(id, oracle_id, name, overrides \\ %{}) do
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
      |> Map.merge(overrides)
      |> CardData.from_scryfall()
      |> Map.delete(:selection_key)

    Repo.insert_all(TheGathering.Catalog.Card, [attrs])
  end
end
