defmodule TheGatheringWeb.API.CardControllerTest do
  use TheGatheringWeb.ConnCase

  alias TheGathering.Catalog.Sync

  @fixture Path.expand("../../../support/fixtures/scryfall_catalog.jsonl", __DIR__)

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
                 "can_be_commander" => false
               }
             ]
           } = json_response(conn, 200)
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
end
