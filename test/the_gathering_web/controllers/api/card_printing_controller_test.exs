defmodule TheGatheringWeb.API.CardPrintingControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.{Catalog, Games, Repo, Stats}
  alias TheGathering.Catalog.{Card, CardData, Printing, Sync}

  setup :register_and_log_in_user

  setup %{user: user} do
    Application.put_env(:the_gathering, :scryfall_req_options, plug: {Req.Test, __MODULE__})
    Application.put_env(:the_gathering, :scryfall_search_limit, 1_000_000)

    on_exit(fn ->
      Application.delete_env(:the_gathering, :scryfall_req_options)
      Application.delete_env(:the_gathering, :scryfall_search_limit)
    end)

    for {id, name} <- [{"commander", "Tymna the Weaver"}, {"partner", "Thrasios, Triton Hero"}] do
      attrs =
        id
        |> scryfall_card(name)
        |> CardData.from_scryfall()
        |> Map.delete(:selection_key)

      Repo.insert!(struct!(Card, attrs))

      Repo.insert!(%Printing{
        id: "#{id}-alternate",
        oracle_id: "oracle-#{id}",
        name: name,
        set_code: "old",
        set_name: "Original Set",
        collector_number: "42",
        image_uris: %{"art_crop" => "https://img.example/#{id}-alternate.jpg"}
      })
    end

    {:ok, player} = Games.create_player(%{name: "Printing owner"})
    {:ok, player} = Games.link_player_to_user(player, user)

    {:ok, deck} =
      Games.create_deck(%{
        player_id: player.id,
        name: "Partners",
        commander_card_id: "commander",
        commander_name: "Tymna the Weaver",
        partner_card_id: "partner",
        partner_name: "Thrasios, Triton Hero",
        color_identity: "WUBG"
      })

    %{deck: deck, player: player}
  end

  test "lists and caches English paper printings by oracle identity with pagination and front-face images",
       %{conn: conn} do
    Req.Test.expect(__MODULE__, fn conn ->
      params = Plug.Conn.fetch_query_params(conn).query_params
      assert params["q"] == "oracleid:oracle-commander game:paper lang:en"
      assert params["unique"] == "prints"
      refute params["include_multilingual"]
      assert params["page"] == "2"
      assert get_req_header(conn, "user-agent") != []

      printing =
        scryfall_card("commander", "Tymna the Weaver")
        |> Map.put("id", "double-faced-print")
        |> Map.delete("image_uris")
        |> Map.put("card_faces", [
          %{"image_uris" => %{"art_crop" => "https://img.example/front.jpg"}}
        ])

      other = scryfall_card("partner", "Thrasios, Triton Hero")
      digital = printing |> Map.put("id", "digital") |> Map.put("games", ["arena"])
      japanese = printing |> Map.put("id", "japanese") |> Map.put("lang", "ja")
      Req.Test.json(conn, %{data: [printing, other, digital, japanese], has_more: true})
    end)

    body = conn |> get(~p"/api/card-printings?card_id=commander&page=2") |> json_response(200)

    assert %{"data" => [%{"id" => "double-faced-print", "lang" => "en"}], "has_more" => true} =
             body

    assert Catalog.get_printing("double-faced-print").image_uris["art_crop"] ==
             "https://img.example/front.jpg"

    refute Catalog.get_printing("digital")
    refute Catalog.get_printing("japanese")
    assert Catalog.get_card("double-faced-print") == nil

    assert conn
           |> get(~p"/api/card-printings/double-faced-print")
           |> json_response(200)
           |> get_in(["data", "set_code"]) == "new"
  end

  test "fetches full printing details by Scryfall id and caches the printing", %{conn: conn} do
    Req.Test.expect(__MODULE__, fn conn ->
      assert conn.request_path == "/cards/saga-print"
      assert get_req_header(conn, "user-agent") != []

      card =
        scryfall_card("saga-print", "Kiora Bests the Sea God")
        |> Map.merge(%{
          "mana_cost" => "{5}{U}{U}",
          "type_line" => "Enchantment — Saga",
          "oracle_text" => "I — Create an 8/8 blue Kraken.",
          "power" => nil,
          "layout" => "saga",
          "rarity" => "mythic",
          "released_at" => "2020-01-24",
          "scryfall_uri" => "https://scryfall.com/card/thb/52",
          "image_uris" => %{
            "small" => "https://img.example/saga-small.jpg",
            "normal" => "https://img.example/saga-normal.jpg",
            "png" => "https://img.example/saga.png"
          }
        })

      Req.Test.json(conn, card)
    end)

    body =
      conn
      |> get(~p"/api/card-printings/saga-print/details")
      |> json_response(200)
      |> Map.fetch!("data")

    assert body["name"] == "Kiora Bests the Sea God"
    assert body["mana_cost"] == "{5}{U}{U}"
    assert body["type_line"] == "Enchantment — Saga"
    assert body["oracle_text"] == "I — Create an 8/8 blue Kraken."
    assert body["set_code"] == "new"
    assert body["set_name"] == "New Set"
    assert body["collector_number"] == "9"
    assert body["layout"] == "saga"

    assert body["image_uris"] == %{
             "small" => "https://img.example/saga-small.jpg",
             "normal" => "https://img.example/saga-normal.jpg"
           }

    refute Map.has_key?(body, "games")
    assert Catalog.get_printing("saga-print").set_name == "New Set"
  end

  test "joins the faces of a double-faced printing and reports misses and outages", %{conn: conn} do
    Req.Test.expect(__MODULE__, fn conn ->
      card =
        scryfall_card("mdfc", "Valki, God of Lies // Tibalt, Cosmic Impostor")
        |> Map.delete("image_uris")
        |> Map.put("layout", "modal_dfc")
        |> Map.put("card_faces", [
          %{
            "name" => "Valki, God of Lies",
            "mana_cost" => "{1}{B}",
            "type_line" => "Legendary Creature — God",
            "oracle_text" => "When Valki enters, each opponent reveals their hand.",
            "power" => "2",
            "toughness" => "1",
            "image_uris" => %{"normal" => "https://img.example/valki.jpg"}
          },
          %{
            "name" => "Tibalt, Cosmic Impostor",
            "mana_cost" => "{5}{B}{R}",
            "type_line" => "Legendary Planeswalker — Tibalt",
            "oracle_text" => "You may play cards exiled with Tibalt.",
            "loyalty" => "5"
          }
        ])

      Req.Test.json(conn, card)
    end)

    body =
      conn
      |> get(~p"/api/card-printings/mdfc/details")
      |> json_response(200)
      |> Map.fetch!("data")

    assert body["mana_cost"] == "{1}{B}"
    assert body["power"] == "2"
    assert body["toughness"] == "1"

    assert body["oracle_text"] ==
             "When Valki enters, each opponent reveals their hand.\n//\nYou may play cards exiled with Tibalt."

    assert body["image_uris"] == %{"normal" => "https://img.example/valki.jpg"}

    Req.Test.expect(__MODULE__, fn conn -> Req.Test.json(%{conn | status: 404}, %{}) end)
    assert conn |> get(~p"/api/card-printings/nope/details") |> json_response(404)

    Req.Test.expect(__MODULE__, fn conn -> Req.Test.json(%{conn | status: 503}, %{}) end)
    assert conn |> get(~p"/api/card-printings/down/details") |> json_response(502)

    assert build_conn() |> get(~p"/api/card-printings/mdfc/details") |> json_response(401)
  end

  test "supports name-only or obsolete catalog references and surfaces upstream failure", %{
    conn: conn
  } do
    Req.Test.expect(__MODULE__, fn conn ->
      assert Plug.Conn.fetch_query_params(conn).query_params["q"] ==
               "oracleid:oracle-commander game:paper lang:en"

      Req.Test.json(%{conn | status: 503}, %{error: "Unavailable"})
    end)

    assert conn
           |> get(~p"/api/card-printings", %{card_id: "obsolete", name: "Tymna the Weaver"})
           |> json_response(502)

    assert conn |> get(~p"/api/card-printings?page=0") |> json_response(400)
    assert conn |> get(~p"/api/card-printings?name=Unknown") |> json_response(404)
    assert conn |> get(~p"/api/card-printings/missing") |> json_response(404)
    assert build_conn() |> get(~p"/api/card-printings?card_id=commander") |> json_response(401)
  end

  test "saves independent commander and partner printings and resolves them on every deck surface",
       ctx do
    body =
      save(ctx, %{
        commander_printing_id: "commander-alternate",
        partner_printing_id: "partner-alternate"
      })

    assert body["commander_card_id"] == "commander"
    assert body["partner_card_id"] == "partner"
    assert body["color_identity"] == "WUBG"
    assert_art(body)

    assert ctx.conn
           |> get(~p"/api/decks/#{ctx.deck.id}")
           |> json_response(200)
           |> Map.fetch!("data")
           |> assert_art()

    assert ctx.conn
           |> get(~p"/api/decks")
           |> json_response(200)
           |> get_in(["data", Access.at(0)])
           |> assert_art()

    assert ctx.conn
           |> get(~p"/api/players/#{ctx.player.id}")
           |> json_response(200)
           |> get_in(["data", "decks", Access.at(0)])
           |> assert_art()

    assert ctx.conn
           |> get(~p"/api/deck-chooser")
           |> json_response(200)
           |> get_in(["data", "deck"])
           |> assert_art()

    {:ok, other} = Games.create_player(%{name: "Other pilot"})

    {:ok, mirror} =
      Games.create_deck(%{
        player_id: other.id,
        name: "Default art",
        commander_card_id: "commander",
        commander_name: "Tymna the Weaver"
      })

    {:ok, game} =
      Games.create_game(%{
        played_at: ~U[2026-09-21 12:00:00Z],
        seats: [
          %{player_id: ctx.player.id, deck_id: ctx.deck.id, seat: 1, result: "win"},
          %{player_id: other.id, deck_id: mirror.id, seat: 2, result: "loss"}
        ]
      })

    seats =
      ctx.conn
      |> get(~p"/api/games/#{game.id}")
      |> json_response(200)
      |> get_in(["data", "seats"])

    selected = Enum.find(seats, &(&1["deck"]["id"] == ctx.deck.id))
    assert_art(selected["deck"])
    default = Enum.find(seats, &(&1["deck"]["id"] == mirror.id))

    assert default["deck"]["commander_art_crop_url"] ==
             "https://img.example/commander-default.jpg"

    assert [%{id: "commander", games: 2, wins: 1}] =
             Enum.filter(Stats.commanders(), &(&1.id == "commander"))

    assert {:ok, imported} =
             Games.find_or_create_deck(ctx.player, "Imported partners", %{
               commander_name: "Thrasios, Triton Hero",
               partner_name: "Tymna the Weaver"
             })

    assert imported.id == ctx.deck.id
    assert imported.commander_printing_id == "commander-alternate"
  end

  test "rejects unknown and mismatched printings atomically, including on create", ctx do
    for attrs <- [
          %{commander_printing_id: "partner-alternate"},
          %{partner_printing_id: "commander-alternate"},
          %{commander_printing_id: "missing"},
          %{commander_name: "Thrasios, Triton Hero", commander_printing_id: "commander-alternate"}
        ] do
      assert ctx.conn
             |> patch(~p"/api/decks/#{ctx.deck.id}", %{
               deck: Map.put(attrs, :name, "Must not change")
             })
             |> json_response(422)
    end

    assert Games.get_deck!(ctx.deck.id).name == "Partners"

    assert {:error, changeset} =
             Games.create_deck(%{
               player_id: ctx.player.id,
               name: "Invalid",
               commander_name: "Tymna the Weaver",
               commander_printing_id: "partner-alternate"
             })

    assert Keyword.has_key?(changeset.errors, :commander_printing_id)
  end

  test "clears defaults explicitly and stale printings when a card changes or partner is removed",
       ctx do
    save(ctx, %{
      commander_printing_id: "commander-alternate",
      partner_printing_id: "partner-alternate"
    })

    unchanged = save(ctx, %{name: "Renamed"})
    assert_art(unchanged)
    default = save(ctx, %{commander_printing_id: nil})
    assert default["commander_art_crop_url"] == "https://img.example/commander-default.jpg"
    assert default["partner_printing_id"] == "partner-alternate"

    save(ctx, %{commander_printing_id: "commander-alternate"})

    changed =
      save(ctx, %{
        commander_card_id: "partner",
        commander_name: "Thrasios, Triton Hero",
        partner_card_id: nil,
        partner_name: nil
      })

    assert changed["commander_printing_id"] == nil
    assert changed["partner_printing_id"] == nil
    assert changed["commander_art_crop_url"] == "https://img.example/partner-default.jpg"
    assert changed["partner_art_crop_url"] == nil
  end

  test "saved printing art survives a catalog replacement and a missing crop falls back", ctx do
    save(ctx, %{
      commander_printing_id: "commander-alternate",
      partner_printing_id: "partner-alternate"
    })

    source = Path.expand("../../../support/fixtures/scryfall_catalog.jsonl", __DIR__)
    assert {:ok, 2} = Sync.run(source: {:file, source})
    refute Catalog.get_card("commander")
    assert_art(save(ctx, %{name: "After sync"}))

    art = Catalog.art_crop_urls([{"printing-latest", "Lightning Bolt"}, {:printing, "missing"}])

    assert Catalog.art_crop_url(art, "printing-latest", nil, "missing") ==
             "https://img.example/latest-art.jpg"
  end

  defp save(ctx, attrs) do
    ctx.conn
    |> patch(~p"/api/decks/#{ctx.deck.id}", %{deck: attrs})
    |> json_response(200)
    |> Map.fetch!("data")
  end

  defp assert_art(deck) do
    assert deck["commander_art_crop_url"] == "https://img.example/commander-alternate.jpg"
    assert deck["partner_art_crop_url"] == "https://img.example/partner-alternate.jpg"
    true
  end

  defp scryfall_card(id, name) do
    %{
      "id" => id,
      "oracle_id" => "oracle-#{id}",
      "name" => name,
      "type_line" => "Legendary Creature",
      "games" => ["paper"],
      "set" => "new",
      "set_name" => "New Set",
      "collector_number" => "9",
      "lang" => "en",
      "image_uris" => %{"art_crop" => "https://img.example/#{id}-default.jpg"}
    }
  end
end
