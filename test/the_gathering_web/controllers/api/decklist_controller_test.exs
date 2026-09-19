defmodule TheGatheringWeb.API.DecklistControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Decklists.Cache

  setup :register_and_log_in_user

  setup do
    Cache.clear()
    Application.put_env(:the_gathering, :decklists_req_options, plug: {Req.Test, __MODULE__})

    on_exit(fn -> Application.delete_env(:the_gathering, :decklists_req_options) end)
  end

  test "POST /api/decklists/resolve returns the decklist envelope", %{conn: conn} do
    Req.Test.stub(__MODULE__, fn conn ->
      Req.Test.json(conn, %{
        "name" => "API Deck",
        "createdByUser" => %{"userName" => "brewer"},
        "boards" => %{
          "commanders" => %{
            "count" => 1,
            "cards" => %{
              "one" => %{
                "quantity" => 1,
                "card" => %{"name" => "Atraxa, Praetors' Voice", "color_identity" => ~w(W U B G)}
              }
            }
          },
          "mainboard" => %{"count" => 99, "cards" => %{}}
        }
      })
    end)

    conn = post(conn, ~p"/api/decklists/resolve", %{url: "https://moxfield.com/decks/api-test"})

    assert %{
             "data" => %{
               "source" => "moxfield",
               "name" => "API Deck",
               "author" => "brewer",
               "card_count" => 100,
               "commanders" => [%{"name" => "Atraxa, Praetors' Voice"}]
             }
           } = json_response(conn, 200)
  end

  test "returns a field error for invalid and unsupported URLs", %{conn: conn} do
    for url <- ["not a URL", "https://example.com/a-deck"] do
      response = post(conn, ~p"/api/decklists/resolve", %{url: url})

      assert json_response(response, 422) == %{
               "errors" => %{"url" => ["is not a supported deck-list URL"]}
             }
    end
  end

  test "maps upstream failures to bad gateway", %{conn: conn} do
    Req.Test.stub(__MODULE__, fn conn -> Plug.Conn.send_resp(conn, 500, "upstream failed") end)

    conn = post(conn, ~p"/api/decklists/resolve", %{url: "https://archidekt.com/decks/123"})

    assert json_response(conn, 502) == %{"errors" => %{"detail" => "Bad Gateway"}}
  end
end
