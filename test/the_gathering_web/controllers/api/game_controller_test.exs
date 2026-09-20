defmodule TheGatheringWeb.API.GameControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Catalog.Card
  alias TheGathering.Games
  alias TheGathering.Repo

  setup :register_and_log_in_user

  setup do
    Repo.insert!(%Card{
      id: "kangee",
      oracle_id: "oracle-kangee",
      name: "Kangee, Sky Warden",
      normalized_name: "kangee, sky warden",
      cmc: 0.0,
      type_line: "Legendary Creature",
      colors: [],
      color_identity: [],
      image_uris: %{"art_crop" => "https://cards.example/kangee-art.jpg"},
      set_code: "tst",
      collector_number: "1",
      layout: "normal",
      rarity: "rare",
      commander_legal: true,
      can_be_commander: true
    })

    Repo.insert!(%Card{
      id: "swan-song",
      oracle_id: "oracle-swan-song",
      name: "Swan Song",
      normalized_name: "swan song",
      cmc: 1.0,
      type_line: "Instant",
      colors: [],
      color_identity: [],
      image_uris: %{"art_crop" => "https://cards.example/swan-song-art.jpg"},
      set_code: "tst",
      collector_number: "2",
      layout: "normal",
      rarity: "rare",
      commander_legal: true,
      can_be_commander: false
    })

    {:ok, alice} = Games.create_player(%{name: "Alice"})
    {:ok, bob} = Games.create_player(%{name: "Bob"})

    {:ok, deck} =
      Games.create_deck(%{
        player_id: alice.id,
        name: "Birds",
        commander_name: "Kangee, Sky Warden"
      })

    %{alice: alice, bob: bob, deck: deck}
  end

  test "POST /api/games creates nested seats and returns the documented shape", %{
    conn: conn,
    user: user,
    alice: alice,
    bob: bob,
    deck: deck
  } do
    payload = %{
      game: %{
        # The creator comes from the session, never from the payload.
        created_by_user_id: user.id + 1000,
        played_at: "2026-09-19T18:30:00Z",
        duration_minutes: 57,
        turns: 9,
        notes: "Close finish",
        seats: [
          %{
            player_id: alice.id,
            deck_id: deck.id,
            seat: 1,
            result: "win",
            mvp_card_id: "swan-song",
            mvp_card_name: "Swan Song"
          },
          %{player_id: bob.id, seat: 2, result: "loss"}
        ]
      }
    }

    response = conn |> post(~p"/api/games", payload) |> json_response(201)

    assert %{
             "data" => %{
               "id" => id,
               "source" => "manual",
               "created_by_user_id" => created_by_user_id,
               "duration_minutes" => 57,
               "seats" => [
                 %{
                   "seat" => 1,
                   "result" => "win",
                   "player" => %{"id" => alice_id, "name" => "Alice"},
                   "deck" => %{"id" => deck_id, "commander_name" => "Kangee, Sky Warden"},
                   "mvp_card_name" => "Swan Song",
                   "mvp_art_crop_url" => "https://cards.example/swan-song-art.jpg"
                 },
                 %{"seat" => 2, "result" => "loss", "player" => %{"name" => "Bob"}, "deck" => nil}
               ]
             }
           } = response

    assert is_integer(id)
    assert created_by_user_id == user.id
    assert alice_id == alice.id
    assert deck_id == deck.id

    assert get_in(response, ["data", "seats", Access.at(0), "deck", "commander_art_crop_url"]) ==
             "https://cards.example/kangee-art.jpg"
  end

  test "POST /api/games requires a signed-in user", %{alice: alice, bob: bob} do
    payload = %{
      game: %{
        played_at: "2026-09-19T18:30:00Z",
        seats: [
          %{player_id: alice.id, seat: 1, result: "win"},
          %{player_id: bob.id, seat: 2, result: "loss"}
        ]
      }
    }

    conn = build_conn() |> post(~p"/api/games", payload)
    assert json_response(conn, 401) == %{"errors" => %{"detail" => "Unauthorized"}}
    assert Games.list_games() == {[], %{page: 1, per_page: 20, total: 0, total_pages: 1}}
  end

  test "GET /api/games/:id returns the same nested resource shape", %{
    conn: conn,
    alice: alice,
    bob: bob,
    deck: deck
  } do
    {:ok, game} =
      Games.create_game(%{
        played_at: ~U[2026-09-19 18:30:00Z],
        seats: [
          %{player_id: alice.id, deck_id: deck.id, seat: 1, result: "win"},
          %{player_id: bob.id, seat: 2, result: "loss"}
        ]
      })

    response = conn |> get(~p"/api/games/#{game.id}") |> json_response(200)
    assert response["data"]["id"] == game.id
    assert Enum.map(response["data"]["seats"], & &1["player"]["name"]) == ["Alice", "Bob"]
    assert hd(response["data"]["seats"])["deck"]["name"] == "Birds"
  end

  test "game history is private to signed-in users" do
    for path <- [~p"/api/games", ~p"/api/players", ~p"/api/decks", ~p"/api/cards?q=a"] do
      conn = build_conn() |> get(path)
      assert json_response(conn, 401) == %{"errors" => %{"detail" => "Unauthorized"}}
    end
  end
end
