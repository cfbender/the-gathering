defmodule TheGatheringWeb.API.PlayerControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.AccountsFixtures
  alias TheGathering.Games

  setup %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    member = AccountsFixtures.user_fixture()
    {:ok, drew} = Games.create_player(%{name: "Drew"})
    {:ok, wax} = Games.create_player(%{name: "waxpoetik"})
    %{conn: conn, admin: admin, member: member, drew: drew, wax: wax}
  end

  test "members cannot claim an account or Discord identity through PATCH", ctx do
    conn = log_in_user(ctx.conn, ctx.member)

    body =
      conn
      |> patch(~p"/api/players/#{ctx.drew.id}", %{
        player: %{name: "Drew!", user_id: ctx.member.id, discord_id: "1"}
      })
      |> json_response(200)

    assert body["data"]["name"] == "Drew!"
    assert body["data"]["user_id"] == nil
    assert Games.get_player!(ctx.drew.id).discord_id == nil
  end

  test "admins merge players; members receive 403", ctx do
    conn = log_in_user(ctx.conn, ctx.member)

    assert %{"errors" => %{"detail" => "Forbidden"}} =
             conn
             |> post(~p"/api/players/#{ctx.wax.id}/merge", %{target_id: ctx.drew.id})
             |> json_response(403)

    conn = log_in_user(recycle(conn), ctx.admin)

    body =
      conn
      |> post(~p"/api/players/#{ctx.wax.id}/merge", %{target_id: ctx.drew.id})
      |> json_response(200)

    assert body["data"]["id"] == ctx.drew.id
    assert Games.get_player(ctx.wax.id) == nil

    assert %{"errors" => %{"detail" => "Not Found"}} =
             conn
             |> recycle()
             |> log_in_user(ctx.admin)
             |> post(~p"/api/players/#{ctx.wax.id}/merge", %{target_id: ctx.drew.id})
             |> json_response(404)
  end

  test "admins link an account to a player and the players list exposes user_id", ctx do
    conn = log_in_user(ctx.conn, ctx.admin)

    body =
      conn
      |> put(~p"/api/admin/users/#{ctx.member.id}/player", %{player_id: ctx.drew.id})
      |> json_response(200)

    assert body["data"]["user_id"] == ctx.member.id

    players =
      conn |> recycle() |> log_in_user(ctx.admin) |> get(~p"/api/players") |> json_response(200)

    assert %{"user_id" => user_id} = Enum.find(players["data"], &(&1["id"] == ctx.drew.id))
    assert user_id == ctx.member.id

    # The same player cannot be handed to a second account.
    other = AccountsFixtures.user_fixture()

    assert %{"errors" => %{"merge" => ["players belong to different accounts"]}} =
             conn
             |> recycle()
             |> log_in_user(ctx.admin)
             |> put(~p"/api/admin/users/#{other.id}/player", %{player_id: ctx.drew.id})
             |> json_response(422)
  end
end
