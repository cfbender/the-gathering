defmodule TheGathering.Games.SyncRemoteDecksTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.{Accounts, AccountsFixtures, Games, Repo}
  alias TheGathering.Catalog.{Card, CardData}
  alias TheGathering.Decklists.Cache
  alias TheGathering.Games.SyncRemoteDecks

  setup do
    Cache.clear()
    Application.put_env(:the_gathering, :decklists_req_options, plug: {Req.Test, __MODULE__})

    on_exit(fn -> Application.delete_env(:the_gathering, :decklists_req_options) end)

    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: "Brewer"}, user.id)

    {:ok, user} =
      Accounts.update_profile(user, %{
        "display_name" => user.display_name,
        "moxfield_username" => "brewer"
      })

    %{user: user, player: player}
  end

  test "links a same-commander deck without renaming it and creates the rest", ctx do
    insert_card("krenko", "Krenko, Mob Boss", ~w(R))

    {:ok, mine} =
      Games.create_deck(%{
        player_id: ctx.player.id,
        name: "Goblins!!",
        commander_name: "krenko, mob boss"
      })

    stub_moxfield([
      moxfield_deck("a", "Krenko Storm", ["Krenko, Mob Boss"], ~w(R)),
      moxfield_deck("b", "Krenko Budget", ["Krenko, Mob Boss"], ~w(R))
    ])

    assert {:ok, %{created: 1, updated: 1, errors: []}} = SyncRemoteDecks.run(ctx.user)

    linked = Games.get_deck!(mine.id)
    assert linked.name == "Goblins!!"
    assert linked.decklist_url == "https://moxfield.com/decks/a"
    assert linked.decklist_source == "moxfield"
    assert linked.commander_card_id == "krenko"
    assert linked.color_identity == "R"

    # The second Krenko list must not steal the link; it becomes its own deck.
    [created] = Enum.reject(Games.list_decks(%{player_id: ctx.player.id}), &(&1.id == mine.id))
    assert created.name == "Krenko Budget"
    assert created.decklist_url == "https://moxfield.com/decks/b"
  end

  test "matches partner pairs in either order and never re-points a linked deck", ctx do
    {:ok, pair} =
      Games.create_deck(%{
        player_id: ctx.player.id,
        name: "Tymna Thrasios",
        commander_name: "Tymna the Weaver",
        partner_name: "Thrasios, Triton Hero"
      })

    {:ok, elsewhere} =
      Games.create_deck(%{
        player_id: ctx.player.id,
        name: "Meren",
        commander_name: "Meren of Clan Nel Toth",
        decklist_url: "https://archidekt.com/decks/9"
      })

    stub_moxfield([
      moxfield_deck("p", "Blue Farm", ["Thrasios, Triton Hero", "Tymna the Weaver"], ~w(W U G)),
      moxfield_deck("m", "Meren Reanimator", ["Meren of Clan Nel Toth"], ~w(B G))
    ])

    assert {:ok, %{created: 1, updated: 1}} = SyncRemoteDecks.run(ctx.user)

    assert Games.get_deck!(pair.id).decklist_url == "https://moxfield.com/decks/p"
    assert Games.get_deck!(elsewhere.id).decklist_url == "https://archidekt.com/decks/9"

    assert Enum.any?(
             Games.list_decks(%{player_id: ctx.player.id}),
             &(&1.name == "Meren Reanimator" and &1.decklist_url == "https://moxfield.com/decks/m")
           )
  end

  test "refreshes a deck already linked by URL from the host", ctx do
    {:ok, old} =
      Games.create_deck(%{
        player_id: ctx.player.id,
        name: "Old name",
        commander_name: "Old commander",
        decklist_url: "https://moxfield.com/decks/a"
      })

    stub_moxfield([moxfield_deck("a", "New name", ["Krenko, Mob Boss"], ~w(R))])

    assert {:ok, %{created: 0, updated: 1}} = SyncRemoteDecks.run(ctx.user)
    assert %{name: "New name", commander_name: "Krenko, Mob Boss"} = Games.get_deck!(old.id)
  end

  test "reports a failed host and syncs the others", ctx do
    {:ok, user} =
      Accounts.update_profile(ctx.user, %{
        "display_name" => ctx.user.display_name,
        "archidekt_username" => "brewer"
      })

    Req.Test.stub(__MODULE__, fn conn ->
      case conn.request_path do
        "/v2/decks/search-sfw" ->
          moxfield_json(conn, [moxfield_deck("a", "Krenko Storm", ["Krenko, Mob Boss"], ~w(R))])

        "/api/decks/v3/" ->
          Plug.Conn.send_resp(conn, 500, "boom")
      end
    end)

    assert {:ok, %{created: 1, updated: 0, errors: [%{source: :archidekt, error: error}]}} =
             SyncRemoteDecks.run(user)

    assert is_binary(error)
  end

  test "rejects users with no deck host configured or no linked player" do
    bare = AccountsFixtures.user_fixture()
    assert {:error, :bad_request} = SyncRemoteDecks.run(bare)

    {:ok, _player} = Games.create_player(%{name: "Unhosted"}, bare.id)
    assert {:error, :bad_request} = SyncRemoteDecks.run(Repo.reload!(bare))
  end

  defp stub_moxfield(decks) do
    Req.Test.stub(__MODULE__, fn conn ->
      assert conn.request_path == "/v2/decks/search-sfw"
      moxfield_json(conn, decks)
    end)
  end

  defp moxfield_json(conn, decks) do
    Req.Test.json(conn, %{"pageNumber" => 1, "totalPages" => 1, "data" => decks})
  end

  defp moxfield_deck(id, name, commanders, colors) do
    %{
      "publicId" => id,
      "name" => name,
      "publicUrl" => "https://moxfield.com/decks/#{id}",
      "commanders" => Enum.map(commanders, &%{"card" => %{"name" => &1}}),
      "colorIdentity" => colors,
      "lastUpdatedAtUtc" => "2026-09-20T10:00:00Z"
    }
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
