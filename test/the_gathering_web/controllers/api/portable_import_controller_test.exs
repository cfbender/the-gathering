defmodule TheGatheringWeb.API.PortableImportControllerTest do
  use TheGatheringWeb.ConnCase, async: false
  import Ecto.Query

  alias TheGathering.Accounts.UserToken
  alias TheGathering.{AccountsFixtures, Games, Repo}

  test "download is versioned, private, and can be previewed and reimported", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    conn = log_in_user(conn, admin)
    {:ok, _} = Games.create_player(%{name: "Unused player"})
    download = get(conn, ~p"/api/exports/portable")
    json = response(download, 200)

    assert %{
             "format" => "the-gathering",
             "version" => 1,
             "players" => [%{"name" => "Unused player"}],
             "games" => []
           } = Jason.decode!(json)

    assert ["no-store"] == get_resp_header(download, "cache-control")
    assert hd(get_resp_header(download, "content-disposition")) =~ ".json"

    assert conn |> post(~p"/api/imports/portable/preview", %{json: json}) |> json_response(200) ==
             %{
               "data" => %{
                 "players" => %{"created" => 0, "reused" => 1},
                 "decks" => %{"created" => 0, "reused" => 0},
                 "games" => %{"created" => 0, "reused" => 0}
               }
             }

    assert %{"data" => %{"players" => %{"reused" => 1}}} =
             conn |> post(~p"/api/imports/portable", %{json: json}) |> json_response(200)
  end

  test "export and preview are admin-only, and committing also requires sudo", %{conn: conn} do
    assert conn |> get(~p"/api/exports/portable") |> json_response(401)
    member = log_in_user(conn, AccountsFixtures.user_fixture())
    assert member |> get(~p"/api/exports/portable") |> json_response(403)
    assert member |> post(~p"/api/imports/portable/preview", %{json: "{}"}) |> json_response(403)
    assert member |> post(~p"/api/imports/portable", %{json: "{}"}) |> json_response(403)

    admin = log_in_user(conn, AccountsFixtures.admin_fixture())
    token = get_session(admin, :user_token)

    Repo.update_all(from(t in UserToken, where: t.token == ^token),
      set: [
        authenticated_at: DateTime.utc_now() |> DateTime.add(-601) |> DateTime.truncate(:second)
      ]
    )

    json = admin |> get(~p"/api/exports/portable") |> response(200)
    assert admin |> post(~p"/api/imports/portable/preview", %{json: json}) |> json_response(200)

    assert %{"errors" => %{"code" => "sudo_required"}} =
             admin |> post(~p"/api/imports/portable", %{json: json}) |> json_response(403)
  end

  test "malformed upload is a normal API error", %{conn: conn} do
    conn = log_in_user(conn, AccountsFixtures.admin_fixture())
    assert conn |> post(~p"/api/imports/portable/preview", %{json: []}) |> json_response(400)

    assert %{"errors" => %{"import" => [_]}} =
             conn
             |> post(~p"/api/imports/portable/preview", %{json: "not JSON"})
             |> json_response(422)
  end
end
