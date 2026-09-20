defmodule TheGatheringWeb.API.CSVImportControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  import Ecto.Query

  alias TheGathering.Accounts.UserToken
  alias TheGathering.AccountsFixtures
  alias TheGathering.Repo

  @csv """
  game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes
  game-1,2026-09-18,Alice,Birds,Kangee,1,win,,60,8,
  game-1,2026-09-18,Bob,Goblins,Krenko,2,loss,,60,8,
  """

  setup %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    %{conn: log_in_user(conn, admin), admin: admin}
  end

  test "admin can preview and commit a CSV import", %{conn: conn, admin: admin} do
    preview = conn |> post(~p"/api/imports/csv/preview", %{csv: @csv}) |> json_response(200)

    assert preview["data"]["valid"]
    assert preview["data"]["players"]["create"] == ["Alice", "Bob"]

    result =
      conn
      |> recycle()
      |> log_in_user(admin)
      |> post(~p"/api/imports/csv", %{csv: @csv})
      |> json_response(200)

    assert %{"created" => 1, "skipped" => 0, "game_ids" => [game_id]} = result["data"]

    game = TheGathering.Games.get_game!(game_id)
    assert game.created_by_user_id == admin.id
  end

  test "commit requires authentication inside the ten-minute sudo window", %{conn: conn} do
    expire_sudo(conn, 602)

    assert conn |> post(~p"/api/imports/csv/preview", %{csv: @csv}) |> json_response(200)

    assert %{"errors" => %{"code" => "sudo_required"}} =
             conn |> post(~p"/api/imports/csv", %{csv: @csv}) |> json_response(403)

    expire_sudo(conn, 598)

    assert %{"data" => %{"created" => 1}} =
             conn |> post(~p"/api/imports/csv", %{csv: @csv}) |> json_response(200)
  end

  test "member receives 403", %{conn: conn} do
    member = AccountsFixtures.user_fixture()
    conn = conn |> recycle() |> log_in_user(member)

    response = conn |> post(~p"/api/imports/csv/preview", %{csv: @csv}) |> json_response(403)
    assert response == %{"errors" => %{"detail" => "Forbidden"}}
  end

  test "sample endpoint downloads a native template", %{conn: conn} do
    conn = get(conn, ~p"/api/imports/csv/sample")
    assert response(conn, 200) =~ "game_id,date,player,deck,commander"
    assert get_resp_header(conn, "content-disposition") |> hd() =~ "the-gathering-games.csv"
  end

  defp expire_sudo(conn, seconds_ago) do
    token = get_session(conn, :user_token)

    Repo.update_all(
      from(user_token in UserToken, where: user_token.token == ^token),
      set: [
        authenticated_at:
          DateTime.utc_now() |> DateTime.add(-seconds_ago, :second) |> DateTime.truncate(:second)
      ]
    )
  end
end
