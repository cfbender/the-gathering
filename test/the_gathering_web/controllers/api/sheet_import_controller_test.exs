defmodule TheGatheringWeb.API.SheetImportControllerTest do
  use TheGatheringWeb.ConnCase, async: false
  import Ecto.Query
  alias TheGathering.Accounts.UserToken
  alias TheGathering.{AccountsFixtures, Repo}

  @text "Date\tWinner\tDeck\tA\tWin Con\tOther Decks\tNotes\n3/7/25\tA\tBirds\t1\tCombat\tB (Goblins)\tNote\n"

  test "admin can preview and commit explicitly selected creates", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    conn = log_in_user(conn, admin)

    params = %{
      "text" => @text,
      "players" => %{"A" => "new", "B" => "new"},
      "decks" => %{
        Jason.encode!(["A", "Birds"]) => "new",
        Jason.encode!(["B", "Goblins"]) => "new"
      }
    }

    initial = conn |> post(~p"/api/imports/sheet/preview", params) |> json_response(200)
    [row] = initial["data"]["rows"]
    assert row["action"] == "skip"
    params = Map.put(params, "actions", %{row["key"] => "create"})
    preview = conn |> post(~p"/api/imports/sheet/preview", params) |> json_response(200)
    params = Map.put(params, "revision", preview["data"]["revision"])

    assert %{"data" => %{"created" => 1}} =
             conn |> post(~p"/api/imports/sheet", params) |> json_response(200)

    assert %{"errors" => %{"import" => [_]}} =
             conn |> post(~p"/api/imports/sheet", params) |> json_response(422)
  end

  test "preview and commit require admin; commit additionally requires sudo", %{conn: conn} do
    assert conn |> post(~p"/api/imports/sheet/preview", %{text: @text}) |> json_response(401)
    member_conn = log_in_user(conn, AccountsFixtures.user_fixture())

    assert member_conn
           |> post(~p"/api/imports/sheet/preview", %{text: @text})
           |> json_response(403)

    assert member_conn |> post(~p"/api/imports/sheet", %{text: @text}) |> json_response(403)
    admin_conn = log_in_user(conn, AccountsFixtures.admin_fixture())
    token = get_session(admin_conn, :user_token)

    Repo.update_all(from(t in UserToken, where: t.token == ^token),
      set: [
        authenticated_at: DateTime.utc_now() |> DateTime.add(-601) |> DateTime.truncate(:second)
      ]
    )

    assert admin_conn
           |> post(~p"/api/imports/sheet/preview", %{text: @text})
           |> json_response(200)

    assert %{"errors" => %{"code" => "sudo_required"}} =
             admin_conn |> post(~p"/api/imports/sheet", %{text: @text}) |> json_response(403)
  end

  test "malformed selections return errors rather than crashing", %{conn: conn} do
    conn = log_in_user(conn, AccountsFixtures.admin_fixture())

    for input <- [
          %{},
          %{text: 42},
          %{text: @text, players: []},
          %{text: @text, decks: %{"x" => []}}
        ] do
      assert conn |> post(~p"/api/imports/sheet/preview", input) |> json_response(400)
    end

    assert %{"errors" => %{"import" => [_]}} =
             conn |> post(~p"/api/imports/sheet/preview", %{text: "bad"}) |> json_response(422)
  end
end
