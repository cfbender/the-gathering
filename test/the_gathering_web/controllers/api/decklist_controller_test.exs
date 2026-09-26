defmodule TheGatheringWeb.API.DecklistControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Catalog.{Card, CardData}
  alias TheGathering.Decklists.Cache
  alias TheGathering.{Games, Repo}

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

  describe "GET /api/decks/:deck_id/decklist" do
    setup do
      {:ok, player} = Games.create_player(%{name: "Brewer"})

      {:ok, deck} =
        Games.create_deck(%{
          player_id: player.id,
          name: "Wilson",
          commander_name: "Wilson, Refined Grizzly",
          decklist_url: "https://archidekt.com/decks/24907541/slug"
        })

      %{deck: deck, player: player}
    end

    test "returns the playable list with catalog details and images", %{conn: conn, deck: deck} do
      insert_card!(%{
        "id" => "catalog-forest",
        "oracle_id" => "oracle-forest",
        "name" => "Forest",
        "type_line" => "Basic Land — Forest",
        "image_uris" => %{
          "small" =>
            "https://cards.scryfall.io/small/front/1/2/12345678-1234-1234-1234-123456789abc.jpg",
          "normal" =>
            "https://cards.scryfall.io/normal/front/1/2/12345678-1234-1234-1234-123456789abc.jpg"
        }
      })

      insert_card!(%{
        "id" => "catalog-other",
        "oracle_id" => "oracle-other",
        "name" => "Other Cards",
        "type_line" => "Creature — Bear",
        "mana_cost" => "{1}{G}",
        "image_uris" => %{
          "small" =>
            "https://cards.scryfall.io/small/front/a/b/abcdef00-1234-1234-1234-123456789abc.jpg"
        }
      })

      Req.Test.stub(__MODULE__, fn conn ->
        Req.Test.json(conn, fixture("archidekt_background.json"))
      end)

      conn = get(conn, ~p"/api/decks/#{deck.id}/decklist")

      assert %{
               "data" => %{
                 "source" => "archidekt",
                 "url" => "https://archidekt.com/decks/24907541",
                 "name" => "I am a public servant",
                 "cards" => cards
               }
             } = json_response(conn, 200)

      assert Enum.map(cards, &{&1["name"], &1["quantity"], &1["zone"]}) == [
               {"Noble Heritage", 1, "commander"},
               {"Wilson, Refined Grizzly", 1, "commander"},
               {"Forest", 38, "mainboard"},
               {"Other cards", 60, "mainboard"}
             ]

      forest = Enum.find(cards, &(&1["name"] == "Forest"))
      assert forest["card_id"] == "catalog-forest"
      assert forest["type_line"] == "Basic Land — Forest"
      assert forest["printing_id"] == "0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e03"

      # The list's printing wins over the catalog's image.
      assert forest["image_uris"]["normal"] ==
               "/api/card-images?" <>
                 URI.encode_query(%{
                   url:
                     "https://cards.scryfall.io/normal/front/0/c/0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e03.jpg"
                 })

      # No printing on the list: fall back to the catalog card, matched case-insensitively.
      other = Enum.find(cards, &(&1["name"] == "Other cards"))
      assert other["card_id"] == "catalog-other"
      assert other["mana_cost"] == "{1}{G}"
      assert other["image_uris"]["small"] =~ "abcdef00-1234-1234-1234-123456789abc"

      # Unknown to the catalog: listed with no details.
      heritage = Enum.find(cards, &(&1["name"] == "Noble Heritage"))
      assert heritage["card_id"] == nil
      assert heritage["type_line"] == nil
    end

    test "404s for decks without a supported link", %{conn: conn, player: player} do
      {:ok, unlinked} =
        Games.create_deck(%{player_id: player.id, name: "Plain", commander_name: "Plain"})

      {:ok, other} =
        Games.create_deck(%{
          player_id: player.id,
          name: "Elsewhere",
          commander_name: "Elsewhere",
          decklist_url: "https://example.com/my-deck"
        })

      for deck_id <- [unlinked.id, other.id, 0] do
        assert json_response(get(conn, ~p"/api/decks/#{deck_id}/decklist"), 404)
      end
    end

    test "maps private lists to 404 and upstream failures to 502", %{conn: conn, deck: deck} do
      Req.Test.stub(__MODULE__, fn conn -> Plug.Conn.send_resp(conn, 403, "") end)
      assert json_response(get(conn, ~p"/api/decks/#{deck.id}/decklist"), 404)

      Req.Test.stub(__MODULE__, fn conn -> Plug.Conn.send_resp(conn, 500, "") end)
      assert json_response(get(conn, ~p"/api/decks/#{deck.id}/decklist"), 502)
    end
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

  defp fixture(name) do
    Path.expand("../../../support/fixtures/decklists/#{name}", __DIR__)
    |> File.read!()
    |> Jason.decode!()
  end
end
