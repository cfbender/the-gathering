defmodule TheGatheringWeb.API.AdminPlayerControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  import Ecto.Query

  alias TheGathering.{Accounts, AccountsFixtures, Games, Repo}
  alias TheGathering.Accounts.UserToken
  alias TheGathering.Games.Player

  setup %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    %{conn: log_in_user(conn, admin), admin: admin}
  end

  test "lists archived and orphaned identities with pagination and search", %{conn: conn} do
    user = AccountsFixtures.user_fixture(%{"username" => "linked_member"})
    {:ok, linked} = Games.create_player(%{name: "Alpha", discord_id: "111"}, user.id)

    {:ok, archived} =
      Games.create_player(%{
        name: "Beta",
        discord_id: "222",
        archived_at: ~U[2026-09-20 00:00:00Z]
      })

    {:ok, _guest} = Games.create_player(%{name: "Gamma"})

    body = conn |> get(~p"/api/admin/players?per_page=1&page=2") |> json_response(200)

    assert [%{"id" => id, "discord_id" => "222", "user" => nil, "archived_at" => archived_at}] =
             body["data"]

    assert id == archived.id
    assert archived_at != nil
    assert body["meta"] == %{"page" => 2, "per_page" => 1, "total" => 3, "total_pages" => 3}

    for search <- ["ALPHA", "111", "LINKED_MEMBER"] do
      body = conn |> get(~p"/api/admin/players?search=#{search}") |> json_response(200)

      assert [%{"id" => id, "user" => %{"id" => user_id, "username" => "linked_member"}}] =
               body["data"]

      assert id == linked.id
      assert user_id == user.id
    end

    assert conn
           |> get(~p"/api/admin/players?search=absent")
           |> json_response(200)
           |> Map.fetch!("data") == []
  end

  test "unlinking an orphaned Discord identity permits merging while retaining history", %{
    conn: conn
  } do
    {:ok, source} = Games.create_player(%{name: "Imported", discord_id: "old-discord"})
    {:ok, target} = Games.create_player(%{name: "Correct", discord_id: "correct-discord"})
    {:ok, opponent} = Games.create_player(%{name: "Opponent"})

    {:ok, deck} =
      Games.create_deck(%{player_id: source.id, name: "Original deck", commander_name: "Alela"})

    {:ok, game} =
      Games.create_game(%{
        played_at: ~U[2026-09-20 18:00:00Z],
        seats: [
          %{player_id: source.id, deck_id: deck.id, seat: 1, result: "win"},
          %{player_id: opponent.id, seat: 2, result: "loss"}
        ]
      })

    assert conn
           |> post(~p"/api/players/#{source.id}/merge", %{target_id: target.id})
           |> json_response(422)

    assert conn |> delete(~p"/api/admin/players/#{source.id}/identity") |> response(204)
    assert Games.get_player(source.id).discord_id == nil
    assert Games.get_deck(deck.id).player_id == source.id
    assert Enum.any?(Games.get_game!(game.id).seats, &(&1.player_id == source.id))

    assert conn
           |> post(~p"/api/players/#{source.id}/merge", %{target_id: target.id})
           |> json_response(200)

    assert Games.get_player(target.id).discord_id == "correct-discord"
    assert Games.get_deck(deck.id).player_id == target.id
    assert Enum.any?(Games.get_game!(game.id).seats, &(&1.player_id == target.id))
  end

  test "unlinking detaches the account without breaking subsequent Discord sign-in", %{conn: conn} do
    {:ok, _} = Accounts.update_settings(%{registration_enabled: true})
    claims = %{"sub" => "linked-discord", "preferred_username" => "linked_member"}
    {:ok, user} = Accounts.sign_in_with_discord(claims)
    player = Repo.get_by!(Player, user_id: user.id)

    assert conn |> delete(~p"/api/admin/players/#{player.id}/identity") |> response(204)
    assert Games.get_player(player.id).user_id == nil
    assert Games.get_player(player.id).discord_id == nil
    assert Accounts.get_user(user.id).discord_id == "linked-discord"
    assert {:ok, signed_in} = Accounts.sign_in_with_discord(claims)
    assert signed_in.id == user.id
    refute Repo.get_by!(Player, user_id: user.id).id == player.id
    assert Games.get_player(player.id).user_id == nil
    assert conn |> delete(~p"/api/admin/players/#{player.id}/identity") |> response(204)
  end

  test "requires admin and recent sudo authentication; missing players return 404", %{
    conn: conn,
    admin: admin
  } do
    {:ok, player} = Games.create_player(%{name: "Protected", discord_id: "keep"})
    member_conn = log_in_user(build_conn(), AccountsFixtures.user_fixture())
    assert member_conn |> get(~p"/api/admin/players") |> json_response(403)

    assert member_conn
           |> delete(~p"/api/admin/players/#{player.id}/identity")
           |> json_response(403)

    assert conn |> delete(~p"/api/admin/players/0/identity") |> json_response(404)

    token = get_session(conn, :user_token)

    Repo.update_all(from(t in UserToken, where: t.token == ^token),
      set: [
        authenticated_at:
          DateTime.utc_now() |> DateTime.add(-11, :minute) |> DateTime.truncate(:second)
      ]
    )

    for response <- [
          get(conn, ~p"/api/admin/players"),
          delete(conn, ~p"/api/admin/players/#{player.id}/identity")
        ] do
      assert json_response(response, 403)["errors"]["code"] == "sudo_required"
    end

    assert Games.get_player(player.id).discord_id == "keep"
    assert Accounts.get_user(admin.id)
  end
end
