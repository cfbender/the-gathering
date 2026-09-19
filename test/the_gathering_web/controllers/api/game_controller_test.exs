defmodule TheGatheringWeb.API.GameControllerTest do
  use TheGatheringWeb.ConnCase, async: true

  alias TheGathering.Games

  setup do
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
    alice: alice,
    bob: bob,
    deck: deck
  } do
    payload = %{
      game: %{
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
               "duration_minutes" => 57,
               "seats" => [
                 %{
                   "seat" => 1,
                   "result" => "win",
                   "player" => %{"id" => alice_id, "name" => "Alice"},
                   "deck" => %{"id" => deck_id, "commander_name" => "Kangee, Sky Warden"},
                   "mvp_card_name" => "Swan Song"
                 },
                 %{"seat" => 2, "result" => "loss", "player" => %{"name" => "Bob"}, "deck" => nil}
               ]
             }
           } = response

    assert is_integer(id)
    assert alice_id == alice.id
    assert deck_id == deck.id
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
end
