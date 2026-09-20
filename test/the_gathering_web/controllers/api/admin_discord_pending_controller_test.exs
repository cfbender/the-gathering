defmodule TheGatheringWeb.API.AdminDiscordPendingControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.AccountsFixtures
  alias TheGathering.Discord.{GameReport, PendingGame}
  alias TheGathering.Games.Game
  alias TheGathering.Repo

  setup %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    %{conn: log_in_user(conn, admin), admin: admin}
  end

  test "admin lists and resolves a pending Discord game", %{conn: conn, admin: admin} do
    {:ok, pending} = PendingGame.upsert(report())

    response = conn |> get(~p"/api/admin/discord/pending") |> json_response(200)

    assert %{
             "data" => [
               %{
                 "id" => pending_id,
                 "external_id" => "spellbot:SB12345",
                 "channel_id" => "444",
                 "players" => players
               }
             ]
           } = response

    assert pending_id == pending.id
    assert Enum.map(players, & &1["display_name"]) == ["Aria", "Bryn"]

    conn =
      conn
      |> recycle()
      |> log_in_user(admin)
      |> patch(~p"/api/admin/discord/pending/#{pending.id}", %{
        winner_discord_id: "222"
      })

    assert response(conn, 204)
    assert Repo.get(PendingGame, pending.id) == nil

    game = Repo.get_by!(Game, source: "discord", external_id: "spellbot:SB12345")
    game = TheGathering.Games.get_game!(game.id)
    assert Enum.find(game.seats, &(&1.player.discord_id == "222")).result == "win"
  end

  test "admin can discard a pending game", %{conn: conn} do
    {:ok, pending} = PendingGame.upsert(report())

    conn = delete(conn, ~p"/api/admin/discord/pending/#{pending.id}")

    assert response(conn, 204)
    assert Repo.get(PendingGame, pending.id) == nil
  end

  defp report do
    %GameReport{
      external_id: "spellbot:SB12345",
      source: "discord",
      played_at: ~U[2026-09-20 14:00:00Z],
      guild_id: "333",
      channel_id: "444",
      players: [
        %{discord_id: "111", display_name: "Aria", commander_name: "Alela"},
        %{discord_id: "222", display_name: "Bryn", commander_name: nil}
      ],
      winner_discord_ids: [],
      raw: %{message_id: "555"}
    }
  end
end
