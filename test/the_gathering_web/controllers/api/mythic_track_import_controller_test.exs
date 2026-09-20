defmodule TheGatheringWeb.API.MythicTrackImportControllerTest do
  use TheGatheringWeb.ConnCase, async: true

  alias TheGathering.AccountsFixtures

  @json Jason.encode!([
          %{
            "id" => "8f3a0a44-0000-4000-8000-00000000abcd",
            "createdOn" => "2026-03-14T19:30:15",
            "gameStatus" => 3,
            "totalTurns" => 9,
            "gameTimeInMinutes" => 55,
            "notes" => nil,
            "players" => [
              %{
                "player" => %{"name" => "Alice"},
                "commander" => %{"name" => "Kangee, Sky Warden", "colors" => ["W", "U"]},
                "turnOrder" => 1,
                "isWinner" => true
              },
              %{
                "player" => %{"name" => "Bob"},
                "commander" => %{"name" => "Krenko, Mob Boss", "colors" => ["R"]},
                "turnOrder" => 2,
                "isWinner" => false
              }
            ]
          }
        ])

  setup %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    %{conn: log_in_user(conn, admin), admin: admin}
  end

  test "admin can preview and commit a Mythic Track export", %{conn: conn, admin: admin} do
    preview =
      conn |> post(~p"/api/imports/mythic_track/preview", %{json: @json}) |> json_response(200)

    assert preview["data"]["valid"]
    assert preview["data"]["warnings"] == []
    assert preview["data"]["players"]["create"] == ["Alice", "Bob"]
    assert [%{"seats" => [%{"color_identity" => "WU"} | _rest]}] = preview["data"]["games"]

    result =
      conn
      |> recycle()
      |> log_in_user(admin)
      |> post(~p"/api/imports/mythic_track", %{json: @json})
      |> json_response(200)

    assert %{"created" => 1, "skipped" => 0, "game_ids" => [game_id]} = result["data"]

    game = TheGathering.Games.get_game!(game_id)
    assert game.source == "mythic_track"
    assert game.created_by_user_id == admin.id
  end

  test "invalid export returns 422 with the preview", %{conn: conn} do
    response =
      conn |> post(~p"/api/imports/mythic_track", %{json: "[]"}) |> json_response(422)

    assert response["data"]["valid"] == false
    assert [%{"field" => "json"}] = response["data"]["errors"]
  end

  test "member receives 403", %{conn: conn} do
    member = AccountsFixtures.user_fixture()
    conn = conn |> recycle() |> log_in_user(member)

    response =
      conn |> post(~p"/api/imports/mythic_track/preview", %{json: @json}) |> json_response(403)

    assert response == %{"errors" => %{"detail" => "Forbidden"}}
  end
end
