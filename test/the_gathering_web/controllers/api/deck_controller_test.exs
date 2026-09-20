defmodule TheGatheringWeb.API.DeckControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.AccountsFixtures
  alias TheGathering.{Games, Stats}

  setup %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    owner = AccountsFixtures.user_fixture()
    other = AccountsFixtures.user_fixture()

    {:ok, owner_player} = Games.create_player(%{name: "Owner"})
    {:ok, owner_player} = Games.link_player_to_user(owner_player, owner)
    {:ok, guest} = Games.create_player(%{name: "Guest"})

    {:ok, deck} =
      Games.create_deck(%{player_id: owner_player.id, name: "Krenko", commander_name: "Krenko"})

    {:ok, guest_deck} =
      Games.create_deck(%{player_id: guest.id, name: "Tyvar", commander_name: "Tyvar"})

    %{
      conn: conn,
      admin: admin,
      owner: owner,
      other: other,
      owner_player: owner_player,
      guest: guest,
      deck: deck,
      guest_deck: guest_deck
    }
  end

  test "another member cannot edit, delete, or create decks for a linked player", ctx do
    conn = log_in_user(ctx.conn, ctx.other)

    assert %{"errors" => %{"detail" => "Forbidden"}} =
             conn
             |> patch(~p"/api/decks/#{ctx.deck.id}", %{deck: %{name: "Stolen"}})
             |> json_response(403)

    assert Games.get_deck!(ctx.deck.id).name == "Krenko"

    assert conn |> delete(~p"/api/decks/#{ctx.deck.id}") |> json_response(403)
    assert Games.get_deck(ctx.deck.id)

    assert conn
           |> post(~p"/api/decks", %{
             deck: %{player_id: ctx.owner_player.id, name: "Planted", commander_name: "X"}
           })
           |> json_response(403)
  end

  test "the linked member and administrators can edit the deck", ctx do
    body =
      ctx.conn
      |> log_in_user(ctx.owner)
      |> patch(~p"/api/decks/#{ctx.deck.id}", %{deck: %{name: "Krenko, Mob Boss"}})
      |> json_response(200)

    assert body["data"]["name"] == "Krenko, Mob Boss"

    body =
      ctx.conn
      |> log_in_user(ctx.admin)
      |> patch(~p"/api/decks/#{ctx.deck.id}", %{deck: %{name: "Krenko!"}})
      |> json_response(200)

    assert body["data"]["name"] == "Krenko!"
  end

  test "members cannot edit decks of unclaimed guest players", ctx do
    assert %{"errors" => %{"detail" => "Forbidden"}} =
             ctx.conn
             |> log_in_user(ctx.other)
             |> patch(~p"/api/decks/#{ctx.guest_deck.id}", %{deck: %{name: "Tyvar Kell"}})
             |> json_response(403)

    assert Games.get_deck!(ctx.guest_deck.id).name == "Tyvar"
  end

  test "PATCH cannot transfer an owned or guest deck and historical stats remain valid", ctx do
    {:ok, _game} =
      Games.create_game(%{
        played_at: ~U[2026-09-20 12:00:00Z],
        seats: [
          %{player_id: ctx.owner_player.id, deck_id: ctx.deck.id, seat: 1, result: "win"},
          %{player_id: ctx.guest.id, seat: 2, result: "loss"}
        ]
      })

    body =
      ctx.conn
      |> log_in_user(ctx.owner)
      |> patch(~p"/api/decks/#{ctx.deck.id}", %{deck: %{player_id: ctx.guest.id}})
      |> json_response(200)

    assert body["data"]["player_id"] == ctx.owner_player.id
    assert Games.get_deck!(ctx.deck.id).player_id == ctx.owner_player.id
    assert %{player: %{id: owner_id}, record: %{games: 1}} = Stats.deck(ctx.deck.id)
    assert owner_id == ctx.owner_player.id

    assert %{"errors" => %{"detail" => "Forbidden"}} =
             ctx.conn
             |> recycle()
             |> log_in_user(ctx.other)
             |> patch(~p"/api/decks/#{ctx.guest_deck.id}", %{
               deck: %{player_id: ctx.owner_player.id}
             })
             |> json_response(403)

    assert Games.get_deck!(ctx.guest_deck.id).player_id == ctx.guest.id
  end
end
