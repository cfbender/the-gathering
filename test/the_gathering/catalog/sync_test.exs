defmodule TheGathering.Catalog.SyncTest do
  use TheGathering.DataCase

  alias TheGathering.Catalog
  alias TheGathering.Catalog.Sync

  @fixture Path.expand("../../support/fixtures/scryfall_catalog.jsonl", __DIR__)

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
end
