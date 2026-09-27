defmodule TheGatheringWeb.API.V1.GameControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Accounts
  alias TheGathering.Accounts.ApiKey
  alias TheGathering.AccountsFixtures
  alias TheGathering.Games
  alias TheGathering.Repo

  setup do
    user = AccountsFixtures.user_fixture()
    {:ok, {token, api_key}} = Accounts.create_api_key(user, %{"name" => "script"})
    {:ok, me} = Games.create_player(%{name: "Me"}, user.id)
    {:ok, alice} = Games.create_player(%{name: "Alice"})
    {:ok, bob} = Games.create_player(%{name: "Bob"})

    {:ok, early} = Games.create_game(game_attrs(~U[2026-09-01 18:00:00Z], me, alice), nil)
    {:ok, late} = Games.create_game(game_attrs(~U[2026-09-20 18:00:00Z], me, bob), nil)
    {:ok, others} = Games.create_game(game_attrs(~U[2026-09-21 03:00:00Z], alice, bob), nil)

    %{
      user: user,
      token: token,
      api_key: api_key,
      me: me,
      alice: alice,
      games: %{early: early, late: late, others: others}
    }
  end

  defp authed(token), do: put_req_header(build_conn(), "authorization", "Bearer " <> token)

  defp ids(conn, params) do
    conn
    |> get(~p"/api/v1/games", params)
    |> json_response(200)
    |> Map.fetch!("data")
    |> Enum.map(& &1["id"])
  end

  test "lists every game the owner can see, newest first, with pagination", %{
    token: token,
    games: games
  } do
    response = authed(token) |> get(~p"/api/v1/games") |> json_response(200)

    assert Enum.map(response["data"], & &1["id"]) == [
             games.others.id,
             games.late.id,
             games.early.id
           ]

    assert response["pagination"] == %{
             "page" => 1,
             "per_page" => 20,
             "total" => 3,
             "total_pages" => 1
           }
  end

  test "filters by the owner's linked player with player_id=me", %{token: token, games: games} do
    assert ids(authed(token), %{player_id: "me"}) == [games.late.id, games.early.id]
  end

  test "filters by player ID and inclusive local dates", %{
    token: token,
    alice: alice,
    games: games
  } do
    assert ids(authed(token), %{player_id: alice.id}) == [games.others.id, games.early.id]

    assert ids(authed(token), %{date_from: "2026-09-02", date_to: "2026-09-20"}) ==
             [games.late.id]

    # 2026-09-21 03:00 UTC is still 2026-09-20 in New York.
    assert ids(authed(token), %{
             date_from: "2026-09-20",
             date_to: "2026-09-20",
             tz: "America/New_York"
           }) == [games.others.id, games.late.id]
  end

  test "rejects invalid filters instead of widening the result", %{token: token} do
    for params <- [
          %{player_id: "abc"},
          %{player_id: "0"},
          %{date_from: "yesterday"},
          %{date_to: "2026-13-01"},
          %{tz: "Mars/Olympus"},
          %{per_page: "-1"}
        ] do
      assert authed(token) |> get(~p"/api/v1/games", params) |> json_response(400),
             "expected 400 for #{inspect(params)}"
    end
  end

  test "player_id=me is not found when the owner has no linked player" do
    {:ok, {token, _key}} =
      Accounts.create_api_key(AccountsFixtures.user_fixture(), %{"name" => "unlinked"})

    assert authed(token) |> get(~p"/api/v1/games", %{player_id: "me"}) |> json_response(404)
  end

  test "records when a key was last used", %{token: token, api_key: api_key} do
    assert authed(token) |> get(~p"/api/v1/games") |> json_response(200)
    assert %DateTime{} = Repo.get!(ApiKey, api_key.id).last_used_at
  end

  test "rejects missing, malformed, unknown, and revoked keys", %{
    user: user,
    token: token,
    api_key: api_key
  } do
    assert %{"errors" => %{"detail" => "Unauthorized"}} =
             build_conn() |> get(~p"/api/v1/games") |> json_response(401)

    assert authed("tg_not-a-real-key") |> get(~p"/api/v1/games") |> json_response(401)

    assert build_conn()
           |> put_req_header("authorization", "Basic " <> token)
           |> get(~p"/api/v1/games")
           |> json_response(401)

    {:ok, _key} = Accounts.delete_api_key(user, api_key.id)
    assert authed(token) |> get(~p"/api/v1/games") |> json_response(401)
  end

  test "keys stop working as soon as the owner is disabled", %{user: user, token: token} do
    {:ok, _user} = Accounts.disable_user(user)
    assert authed(token) |> get(~p"/api/v1/games") |> json_response(401)
  end

  test "keys do not authenticate session-only API routes", %{token: token, me: me, alice: alice} do
    assert authed(token) |> get(~p"/api/games") |> json_response(401)
    assert authed(token) |> get(~p"/api/session/api-keys") |> json_response(401)

    assert authed(token)
           |> post(~p"/api/games", %{game: game_attrs(DateTime.utc_now(), me, alice)})
           |> json_response(401)
  end

  defp game_attrs(played_at, winner, loser) do
    %{
      played_at: played_at,
      seats: [
        %{player_id: winner.id, seat: 1, result: "win"},
        %{player_id: loser.id, seat: 2, result: "loss"}
      ]
    }
  end
end
