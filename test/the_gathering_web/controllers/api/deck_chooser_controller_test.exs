defmodule TheGatheringWeb.API.DeckChooserControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Accounts
  alias TheGathering.AccountsFixtures
  alias TheGathering.Catalog.{Card, CardData}
  alias TheGathering.Decklists.Cache
  alias TheGathering.Games
  alias TheGathering.Repo

  setup %{conn: conn} do
    Cache.clear()
    Application.put_env(:the_gathering, :decklists_req_options, plug: {Req.Test, __MODULE__})

    # Personal ManaVault hosts are resolved and checked against private ranges before
    # every request, so give the fake host a public address.
    Application.put_env(:the_gathering, :decklists_dns_resolver, fn _host, family ->
      if family == :inet, do: {:ok, [{93, 184, 216, 34}]}, else: {:ok, []}
    end)

    on_exit(fn ->
      Application.delete_env(:the_gathering, :decklists_req_options)
      Application.delete_env(:the_gathering, :decklists_dns_resolver)
    end)

    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: "Chooser"}, user.id)
    %{conn: log_in_user(conn, user), user: user, player: player}
  end

  test "records skip and choose outcomes for the signed-in player's deck", ctx do
    {:ok, deck} =
      Games.create_deck(%{
        player_id: ctx.player.id,
        name: "Krenko",
        commander_name: "Krenko"
      })

    assert %{"data" => %{"skip_count" => 1}} =
             ctx.conn
             |> post(~p"/api/deck-chooser/#{deck.id}/outcomes", %{outcome: "skipped"})
             |> json_response(200)

    assert %{"data" => %{"skip_count" => 0}} =
             ctx.conn
             |> recycle()
             |> log_in_user(ctx.user)
             |> post(~p"/api/deck-chooser/#{deck.id}/outcomes", %{outcome: "played"})
             |> json_response(200)
  end

  test "explains when the user has no linked player", %{conn: conn} do
    unlinked = AccountsFixtures.user_fixture()

    assert %{"data" => %{"deck" => nil, "reason" => "player_not_linked"}} =
             conn
             |> log_in_user(unlinked)
             |> get(~p"/api/deck-chooser")
             |> json_response(200)
  end

  test "sync creates and updates ManaVault decks and resolves commanders", ctx do
    insert_card("atraxa", "Atraxa, Praetors' Voice", ~w(W U B G))
    insert_card("krenko", "Krenko, Mob Boss", ~w(R))

    {:ok, existing} =
      Games.create_deck(%{
        player_id: ctx.player.id,
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

    assert %{"data" => %{"created" => 1, "updated" => 1}} =
             ctx.conn
             |> recycle()
             |> log_in_user(user)
             |> post(~p"/api/deck-chooser/sync")
             |> json_response(200)

    updated = Games.get_deck!(existing.id)
    assert updated.name == "Atraxa counters"
    assert updated.commander_card_id == "atraxa"
    assert updated.color_identity == "WUBG"

    [created] =
      Enum.reject(Games.list_decks(%{player_id: ctx.player.id}), &(&1.id == existing.id))

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
