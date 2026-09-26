defmodule TheGatheringWeb.API.CardControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Catalog.{Card, CardData, Sync}
  alias TheGathering.Repo

  @fixture Path.expand("../../../support/fixtures/scryfall_catalog.jsonl", __DIR__)

  setup :register_and_log_in_user

  setup do
    assert {:ok, 2} = Sync.run(source: {:file, @fixture})
    :ok
  end

  test "GET /api/cards searches the local catalog", %{conn: conn} do
    conn = get(conn, ~p"/api/cards?q=Jotun&limit=20")

    assert %{
             "data" => [
               %{
                 "id" => "jotun",
                 "name" => "Jötun Grunt",
                 "image_uris" => %{},
                 "game_changer" => false,
                 "can_be_commander" => false
               }
             ]
           } = json_response(conn, 200)
  end

  test "summaries and details expose Game Changers", %{conn: conn} do
    insert_card!(%{
      "id" => "rhystic",
      "oracle_id" => "oracle-rhystic",
      "name" => "Rhystic Study",
      "game_changer" => true
    })

    assert %{"data" => [%{"game_changer" => true}]} =
             conn |> get(~p"/api/cards?q=Rhystic") |> json_response(200)

    assert %{"data" => %{"game_changer" => true}} =
             conn |> get(~p"/api/cards/rhystic") |> json_response(200)
  end

  test "GET /api/cards partner mode includes Backgrounds without loosening commander mode", %{
    conn: conn
  } do
    insert_card!(%{
      "id" => "background",
      "oracle_id" => "oracle-background",
      "name" => "Master Chef",
      "type_line" => "Legendary Enchantment — Background",
      "oracle_text" => "Commander creatures you own have base power and toughness 3/3.",
      "color_identity" => ["G"]
    })

    conn = get(conn, ~p"/api/cards?q=Master%20Chef&partner=true")

    assert %{"data" => [%{"id" => "background", "commander_pairing" => "background"}]} =
             json_response(conn, 200)

    conn = get(recycle(conn), ~p"/api/cards?q=Master%20Chef&commander=true")
    assert %{"data" => []} = json_response(conn, 200)
  end

  test "GET /api/cards partner mode allows any legendary creature for rule 0 pairings", %{
    conn: conn
  } do
    insert_card!(%{
      "id" => "clara",
      "oracle_id" => "oracle-clara",
      "name" => "Clara Oswald",
      "type_line" => "Legendary Creature — Human Advisor",
      "oracle_text" =>
        "If a triggered ability of a Doctor you control triggers, that ability triggers an additional time.\nDoctor's companion (You can have two commanders if the other is the Doctor.)"
    })

    insert_card!(%{
      "id" => "krenko",
      "oracle_id" => "oracle-krenko",
      "name" => "Krenko, Mob Boss",
      "type_line" => "Legendary Creature — Goblin Warrior",
      "oracle_text" => "{T}: Create X 1/1 red Goblin creature tokens."
    })

    assert %{"data" => [%{"id" => "clara"}]} =
             conn |> get(~p"/api/cards?q=Clara&partner=true") |> json_response(200)

    assert %{"data" => [%{"id" => "krenko"}]} =
             conn |> get(~p"/api/cards?q=Krenko&partner=true") |> json_response(200)

    assert %{"data" => []} =
             conn |> get(~p"/api/cards?q=Jotun&partner=true") |> json_response(200)
  end

  test "GET /api/cards/:id returns detail and 404s missing cards", %{conn: conn} do
    conn = get(conn, ~p"/api/cards/printing-latest")

    assert %{"data" => %{"id" => "printing-latest", "set_code" => "new"}} =
             json_response(conn, 200)

    conn = get(recycle(conn), ~p"/api/cards/missing")
    assert %{"errors" => %{"detail" => "Not Found"}} = json_response(conn, 404)
  end

  test "GET /api/catalog returns sync status", %{conn: conn} do
    conn = get(conn, ~p"/api/catalog")
    assert %{"data" => %{"status" => "succeeded", "card_count" => 2}} = json_response(conn, 200)
  end

  defp insert_card!(overrides) do
    defaults = %{
      "lang" => "en",
      "games" => ["paper"],
      "released_at" => "2024-01-01",
      "set" => "tst",
      "collector_number" => "1",
      "layout" => "normal",
      "rarity" => "common",
      "legalities" => %{"commander" => "legal"}
    }

    attrs =
      defaults |> Map.merge(overrides) |> CardData.from_scryfall() |> Map.delete(:selection_key)

    attrs |> then(&struct!(Card, &1)) |> Repo.insert!()
  end
end
