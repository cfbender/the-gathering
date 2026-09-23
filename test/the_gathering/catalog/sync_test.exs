defmodule TheGathering.Catalog.SyncTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.Catalog
  alias TheGathering.Catalog.Sync

  @fixture Path.expand("../../support/fixtures/scryfall_catalog.jsonl", __DIR__)

  @tag :tmp_dir
  test "publishes and refreshes Game Changer flags through staging and backfill", %{tmp_dir: dir} do
    source = Path.join(dir, "game-changers.jsonl")

    cards = [
      %{
        "id" => "rhystic",
        "oracle_id" => "oracle-rhystic",
        "name" => "Rhystic Study",
        "game_changer" => true
      },
      %{
        "id" => "bolt",
        "oracle_id" => "oracle-bolt",
        "name" => "Lightning Bolt",
        "game_changer" => false
      }
    ]

    File.write!(source, Enum.map_join(cards, "\n", &Jason.encode!/1))
    assert {:ok, 2} = Sync.run(source: {:file, source})
    assert Catalog.get_card!("rhystic").game_changer
    refute Catalog.get_card!("bolt").game_changer
    Catalog.backfill()
    assert Catalog.get_card!("rhystic").game_changer

    File.write!(
      source,
      Enum.map_join(cards, "\n", &Jason.encode!(Map.put(&1, "game_changer", false)))
    )

    assert {:ok, 2} = Sync.run(source: {:file, source})
    refute Catalog.get_card!("rhystic").game_changer
  end

  test "chooses the latest preferred paper printing and is idempotent" do
    assert {:ok, 2} = Sync.run(source: {:file, @fixture})

    card = Catalog.get_card!("printing-latest")
    assert card.oracle_id == "oracle-bolt"
    assert card.set_code == "new"
    assert card.image_uris["normal"] == "https://img.example/latest-normal.jpg"
    assert Catalog.get_card("printing-promo") == nil
    assert Catalog.get_card("printing-digital") == nil

    assert {:ok, 2} = Sync.run(source: {:file, @fixture})
    assert Catalog.count_cards() == 2
    assert Catalog.get_card!("printing-latest").set_code == "new"
  end

  test "records successful and failed state transitions without replacing a good catalog" do
    assert {:ok, 2} = Sync.run(source: {:file, @fixture})
    succeeded = Catalog.sync_status()
    assert succeeded.status == "succeeded"
    assert succeeded.card_count == 2
    assert succeeded.last_started_at
    assert succeeded.last_finished_at
    assert succeeded.last_error == nil

    assert {:error, _reason} = Sync.run(source: {:file, "/does/not/exist.jsonl"})
    failed = Catalog.sync_status()
    assert failed.status == "failed"
    assert failed.last_error =~ "could not stream"
    assert Catalog.count_cards() == 2
  end

  @tag :tmp_dir
  test "rejects an empty generation without replacing a good catalog", %{tmp_dir: tmp_dir} do
    original_cards = sync_good_catalog()
    empty_source = Path.join(tmp_dir, "empty.jsonl")
    File.write!(empty_source, "")

    assert {:error, "staged catalog generation is empty"} =
             Sync.run(source: {:file, empty_source})

    assert_catalog_unchanged(original_cards)
    assert Catalog.sync_status().status == "failed"
    assert Catalog.sync_status().last_error =~ "staged catalog generation is empty"
  end

  @tag :tmp_dir
  test "rejects an all-filtered generation without replacing a good catalog", %{tmp_dir: tmp_dir} do
    original_cards = sync_good_catalog()
    filtered_source = Path.join(tmp_dir, "filtered.jsonl")
    File.write!(filtered_source, Jason.encode!(%{"set_type" => "memorabilia"}) <> "\n")

    assert {:error, "staged catalog generation is empty"} =
             Sync.run(source: {:file, filtered_source})

    assert_catalog_unchanged(original_cards)
  end

  @tag :tmp_dir
  test "does not replace a good catalog when decoding fails after a batch is staged", %{
    tmp_dir: tmp_dir
  } do
    original_cards = sync_good_catalog()
    partial_source = Path.join(tmp_dir, "partial.jsonl")
    first_card = @fixture |> File.stream!() |> Enum.at(0)
    File.write!(partial_source, String.duplicate(first_card, 250) <> "not-json\n")

    assert {:error, reason} = Sync.run(source: {:file, partial_source})
    assert reason =~ "invalid Scryfall bulk JSON"
    assert_catalog_unchanged(original_cards)
  end

  defp sync_good_catalog do
    assert {:ok, 2} = Sync.run(source: {:file, @fixture})
    [Catalog.get_card!("printing-latest"), Catalog.get_card!("jotun")]
  end

  defp assert_catalog_unchanged(original_cards) do
    assert Catalog.count_cards() == 2

    assert Enum.map(original_cards, &Catalog.get_card!(&1.id)) == original_cards
  end
end
