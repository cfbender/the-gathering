defmodule TheGatheringWeb.API.RemoteDeckControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Accounts
  alias TheGathering.Decklists.Cache

  setup :register_and_log_in_user

  setup do
    Cache.clear()
    Application.put_env(:the_gathering, :decklists_req_options, plug: {Req.Test, __MODULE__})

    on_exit(fn -> Application.delete_env(:the_gathering, :decklists_req_options) end)
  end

  test "GET /api/session/remote-decks normalizes configured public deck sources", %{
    conn: conn,
    user: user
  } do
    {:ok, _user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "moxfield_username" => "mox-brewer",
        "archidekt_username" => "arch-brewer"
      })

    Req.Test.stub(__MODULE__, fn conn ->
      conn = Plug.Conn.fetch_query_params(conn)

      case conn.request_path do
        "/v2/decks/search-sfw" ->
          assert conn.query_params["authorUserNames"] == "mox-brewer"

          Req.Test.json(conn, %{
            "pageNumber" => 1,
            "totalPages" => 1,
            "data" => [
              %{
                "publicId" => "mox-id",
                "name" => "Mox Deck",
                "publicUrl" => "https://moxfield.com/decks/mox-id",
                "commanders" => [%{"card" => %{"name" => "Muldrotha, the Gravetide"}}],
                "colorIdentity" => ~w(U B G),
                "lastUpdatedAtUtc" => "2026-09-20T10:00:00Z"
              }
            ]
          })

        "/api/decks/v3/" ->
          assert conn.query_params["ownerUsername"] == "arch-brewer"

          Req.Test.json(conn, %{
            "next" => nil,
            "results" => [
              %{"id" => 42, "name" => "Arch Deck", "updatedAt" => "2026-09-19T10:00:00Z"}
            ]
          })

        "/api/decks/42/" ->
          Req.Test.json(conn, %{
            "cards" => [
              %{
                "categories" => ["Commander"],
                "card" => %{
                  "oracleCard" => %{
                    "name" => "Wilson, Refined Grizzly",
                    "colorIdentity" => ["Green"]
                  }
                }
              },
              %{
                "categories" => ["Commander"],
                "card" => %{
                  "oracleCard" => %{
                    "name" => "Noble Heritage",
                    "colorIdentity" => ["White"]
                  }
                }
              }
            ]
          })
      end
    end)

    conn = get(conn, ~p"/api/session/remote-decks")

    assert %{
             "data" => %{
               "decks" => [
                 %{
                   "source" => "moxfield",
                   "name" => "Mox Deck",
                   "commanders" => ["Muldrotha, the Gravetide"],
                   "color_identity" => ["U", "B", "G"]
                 },
                 %{
                   "source" => "archidekt",
                   "name" => "Arch Deck",
                   "commanders" => ["Wilson, Refined Grizzly", "Noble Heritage"],
                   "color_identity" => ["W", "G"]
                 }
               ],
               "sources" => sources
             }
           } = json_response(conn, 200)

    assert Enum.find(sources, &(&1["source"] == "moxfield")) == %{
             "source" => "moxfield",
             "configured" => true,
             "error" => nil
           }

    assert Enum.find(sources, &(&1["source"] == "manavault"))["configured"] == false
  end

  test "returns clear per-source errors without failing the request", %{conn: conn, user: user} do
    {:ok, _user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "moxfield_username" => "blocked-user",
        "manavault_url" => "https://vault.example.com"
      })

    Req.Test.stub(__MODULE__, fn conn -> Plug.Conn.send_resp(conn, 403, "blocked") end)

    conn = get(conn, ~p"/api/session/remote-decks")
    %{"data" => %{"decks" => [], "sources" => sources}} = json_response(conn, 200)

    assert Enum.find(sources, &(&1["source"] == "moxfield"))["error"] =~ "blocked"
    assert Enum.find(sources, &(&1["source"] == "manavault"))["error"] =~ "public instance"
  end

  test "caches a user's remote deck result", %{conn: conn, user: user} do
    {:ok, _user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "moxfield_username" => "cached-user"
      })

    Req.Test.expect(__MODULE__, 1, fn conn ->
      Req.Test.json(conn, %{"pageNumber" => 1, "totalPages" => 1, "data" => []})
    end)

    conn = get(conn, ~p"/api/session/remote-decks")
    assert %{"data" => %{"decks" => []}} = json_response(conn, 200)

    assert %{"data" => %{"decks" => []}} =
             conn |> recycle() |> get(~p"/api/session/remote-decks") |> json_response(200)
  end
end
