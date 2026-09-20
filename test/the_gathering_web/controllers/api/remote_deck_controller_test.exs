defmodule TheGatheringWeb.API.RemoteDeckControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.{Accounts, Games, Repo}
  alias TheGathering.Catalog.{Card, CardData}
  alias TheGathering.Decklists.{Cache, RemoteDecks}

  setup :register_and_log_in_user

  setup do
    Cache.clear()
    Application.put_env(:the_gathering, :decklists_req_options, plug: {Req.Test, __MODULE__})

    Application.put_env(:the_gathering, :decklists_dns_resolver, fn _host, family ->
      if family == :inet, do: {:ok, [{93, 184, 216, 34}]}, else: {:ok, []}
    end)

    on_exit(fn ->
      Application.delete_env(:the_gathering, :decklists_req_options)
      Application.delete_env(:the_gathering, :decklists_dns_resolver)
      Application.delete_env(:the_gathering, :remote_decks_limits)
    end)
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
      assert conn.host == "93.184.216.34"
      assert Plug.Conn.get_req_header(conn, "host") == ["vault.example.com"]
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

  test "deduplicates concurrent cache misses for one user" do
    user = %TheGathering.Accounts.User{id: -1, moxfield_username: "concurrent-user"}

    Req.Test.expect(__MODULE__, 1, fn conn ->
      Req.Test.json(conn, %{"pageNumber" => 1, "totalPages" => 1, "data" => []})
    end)

    tasks =
      for _index <- 1..2 do
        Task.async(fn ->
          receive do
            :go -> RemoteDecks.list(user)
          end
        end)
      end

    for task <- tasks do
      Req.Test.allow(__MODULE__, self(), task.pid)
      send(task.pid, :go)
    end

    assert Enum.map(tasks, &Task.await/1) |> Enum.uniq() |> length() == 1
  end

  test "does not follow ManaVault redirects", %{conn: conn, user: user} do
    {:ok, _user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "manavault_url" => "https://vault.example.com",
        "manavault_api_key" => "mvk_redirect"
      })

    Req.Test.expect(__MODULE__, 1, fn conn ->
      conn
      |> Plug.Conn.put_resp_header("location", "http://127.0.0.1/admin")
      |> Plug.Conn.send_resp(302, "redirect")
    end)

    %{"data" => %{"sources" => sources}} =
      conn |> get(~p"/api/session/remote-decks") |> json_response(200)

    assert Enum.find(sources, &(&1["source"] == "manavault"))["error"] =~ "could not be reached"
  end

  test "truncates an endpoint that always returns another page", %{conn: conn, user: user} do
    Application.put_env(:the_gathering, :remote_decks_limits, %{max_pages: 2})

    {:ok, _user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "moxfield_username" => "endless"
      })

    Req.Test.expect(__MODULE__, 2, fn conn ->
      conn = Plug.Conn.fetch_query_params(conn)
      page = String.to_integer(conn.query_params["pageNumber"])

      Req.Test.json(conn, %{
        "totalPages" => 1_000,
        "data" => [%{"id" => "deck-#{page}", "name" => "Deck #{page}"}]
      })
    end)

    %{"data" => %{"decks" => decks, "sources" => sources}} =
      conn |> get(~p"/api/session/remote-decks") |> json_response(200)

    assert length(decks) == 2
    assert Enum.find(sources, &(&1["source"] == "moxfield"))["error"] =~ "page limit"
  end

  test "reports response byte and total duration budgets", %{conn: conn, user: user} do
    {:ok, user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "moxfield_username" => "oversized"
      })

    Application.put_env(:the_gathering, :remote_decks_limits, %{max_bytes: 100})

    Req.Test.expect(__MODULE__, 1, fn conn ->
      Req.Test.json(conn, %{
        "totalPages" => 1,
        "data" => [],
        "padding" => String.duplicate("x", 200)
      })
    end)

    %{"data" => %{"sources" => sources}} =
      conn |> get(~p"/api/session/remote-decks") |> json_response(200)

    assert Enum.find(sources, &(&1["source"] == "moxfield"))["error"] =~ "response budget"

    Cache.clear()
    Application.put_env(:the_gathering, :remote_decks_limits, %{duration_ms: 0})

    result = RemoteDecks.list(user)
    assert Enum.find(result.sources, &(&1.source == :moxfield)).error =~ "time budget"
  end

  test "POST /api/session/remote-decks/sync creates and updates ManaVault decks", ctx do
    {:ok, player} = Games.create_player(%{name: "Chooser"}, ctx.user.id)
    insert_card("atraxa", "Atraxa, Praetors' Voice", ~w(W U B G))
    insert_card("krenko", "Krenko, Mob Boss", ~w(R))

    {:ok, existing} =
      Games.create_deck(%{
        player_id: player.id,
        name: "Old name",
        commander_name: "Old commander",
        decklist_url: "https://vault.example.com/decks/1"
      })

    {:ok, user} =
      Accounts.update_profile(ctx.user, %{
        "display_name" => ctx.user.display_name,
        "manavault_url" => "https://vault.example.com",
        "manavault_api_key" => "mvk_test_key"
      })

    Req.Test.expect(__MODULE__, 1, fn conn ->
      assert conn.request_path == "/api/v1/decks"
      assert Plug.Conn.get_req_header(conn, "authorization") == ["Bearer mvk_test_key"]

      Req.Test.json(conn, %{
        "data" => [
          %{
            "id" => 1,
            "name" => "Atraxa counters",
            "commanders" => ["Atraxa, Praetors' Voice"],
            "commanderColorIdentity" => ~w(W U B G),
            "updated_at" => "2026-09-20T12:00:00Z"
          },
          %{
            "id" => 2,
            "name" => "Goblin rush",
            "commanders" => ["Krenko, Mob Boss"],
            "commanderColorIdentity" => ["R"],
            "updated_at" => "2026-09-20T12:00:00Z"
          }
        ],
        "pagination" => %{"total_pages" => 1}
      })
    end)

    assert %{"data" => %{"created" => 1, "updated" => 1, "errors" => []}} =
             ctx.conn
             |> recycle()
             |> log_in_user(user)
             |> post(~p"/api/session/remote-decks/sync")
             |> json_response(200)

    updated = Games.get_deck!(existing.id)
    assert updated.name == "Atraxa counters"
    assert updated.commander_card_id == "atraxa"
    assert updated.color_identity == "WUBG"

    [created] =
      Enum.reject(Games.list_decks(%{player_id: player.id}), &(&1.id == existing.id))

    assert created.name == "Goblin rush"
    assert created.commander_card_id == "krenko"
    assert created.decklist_url == "https://vault.example.com/decks/2"
  end

  defp insert_card(id, name, colors) do
    Repo.insert!(%Card{
      id: id,
      oracle_id: "oracle-#{id}",
      name: name,
      normalized_name: CardData.normalize_name(name),
      color_identity: colors,
      image_uris: %{},
      type_line: "Legendary Creature",
      set_code: "tst",
      collector_number: id,
      layout: "normal",
      rarity: "rare",
      can_be_commander: true
    })
  end
end
