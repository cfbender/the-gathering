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
    assert Enum.find(sources, &(&1["source"] == "manavault"))["error"] =~ "API key"
  end

  test "lists ManaVault decks with the user's API key across pages", %{conn: conn, user: user} do
    {:ok, _user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "manavault_url" => "https://vault.example.com/",
        "manavault_api_key" => "mvk_test_key"
      })

    Req.Test.stub(__MODULE__, fn conn ->
      conn = Plug.Conn.fetch_query_params(conn)
      assert conn.host == "vault.example.com"
      assert conn.request_path == "/api/v1/decks"
      assert Plug.Conn.get_req_header(conn, "authorization") == ["Bearer mvk_test_key"]

      case conn.query_params["page"] do
        "1" ->
          Req.Test.json(conn, %{
            "data" => [
              %{
                "id" => 42,
                "name" => "Muldrotha Reanimator",
                "commanders" => ["Muldrotha, the Gravetide"],
                "commanderColorIdentity" => ["G", "B", "U"],
                "updated_at" => "2026-09-20T14:32:10Z",
                "publicly_shared" => true,
                "public_share_url" => "https://vault.example.com/share/decks/AbCdEf123456"
              }
            ],
            "pagination" => %{"page" => 1, "per_page" => 100, "total" => 2, "total_pages" => 2}
          })

        "2" ->
          Req.Test.json(conn, %{
            "data" => [
              %{
                "id" => 7,
                "name" => "Private brew",
                "commanders" => ["Ardenn, Intrepid Archaeologist", "Kediss, Emberclaw Familiar"],
                "commanderColorIdentity" => ["R", "W"],
                "updated_at" => "2026-09-21T09:00:00Z",
                "publicly_shared" => false,
                "public_share_url" => nil
              }
            ],
            "pagination" => %{"page" => 2, "per_page" => 100, "total" => 2, "total_pages" => 2}
          })
      end
    end)

    conn = get(conn, ~p"/api/session/remote-decks")
    %{"data" => %{"decks" => decks, "sources" => sources}} = json_response(conn, 200)

    assert [
             %{
               "name" => "Private brew",
               "source" => "manavault",
               "commanders" => ["Ardenn, Intrepid Archaeologist", "Kediss, Emberclaw Familiar"],
               "color_identity" => ["W", "R"],
               "url" => "https://vault.example.com/decks/7"
             },
             %{
               "name" => "Muldrotha Reanimator",
               "color_identity" => ["U", "B", "G"],
               "url" => "https://vault.example.com/share/decks/AbCdEf123456"
             }
           ] = decks

    assert Enum.find(sources, &(&1["source"] == "manavault")) == %{
             "source" => "manavault",
             "configured" => true,
             "error" => nil
           }
  end

  test "reports a rejected ManaVault API key without failing the request", %{
    conn: conn,
    user: user
  } do
    {:ok, _user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "manavault_url" => "https://vault.example.com",
        "manavault_api_key" => "mvk_revoked"
      })

    Req.Test.stub(__MODULE__, fn conn ->
      Req.Test.json(%{conn | status: 401}, %{
        "error" => %{"code" => "unauthorized", "message" => "A valid Bearer API key is required"}
      })
    end)

    conn = get(conn, ~p"/api/session/remote-decks")
    %{"data" => %{"decks" => [], "sources" => sources}} = json_response(conn, 200)
    assert Enum.find(sources, &(&1["source"] == "manavault"))["error"] =~ "rejected the API key"
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
