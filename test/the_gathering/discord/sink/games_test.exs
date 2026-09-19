defmodule TheGathering.Discord.Sink.GamesTest do
  use TheGathering.DataCase

  alias TheGathering.Discord.{GameReport, Tracker}
  alias TheGathering.Discord.Sink.Games, as: GamesSink
  alias TheGathering.Games
  alias TheGathering.Games.{Deck, Game, Player}

  test "creates players, decks, and a completed game" do
    assert :ok = GamesSink.handle_report(report(["111"]))

    assert Repo.aggregate(Player, :count) == 2
    assert Repo.aggregate(Deck, :count) == 1
    assert Repo.aggregate(Game, :count) == 1

    [aria, bryn] = Enum.map(["111", "222"], &Repo.get_by!(Player, discord_id: &1))
    game = Repo.get_by!(Game, source: "discord", external_id: "spellbot:SB12345")
    game = Games.get_game!(game.id)

    assert [%{player_id: aria_id, result: "win"}, %{player_id: bryn_id, result: "loss"}] =
             Enum.sort_by(game.seats, & &1.seat)

    assert aria_id == aria.id
    assert bryn_id == bryn.id

    assert %Deck{
             player_id: ^aria_id,
             name: "Alela, Artful Provocateur",
             commander_name: "Alela, Artful Provocateur",
             commander_card_id: nil,
             color_identity: ""
           } = Repo.one(Deck)
  end

  test "replay updates seat order and winner without creating duplicates" do
    assert :ok = GamesSink.handle_report(report(["111"]))

    replay =
      report(["222"])
      |> Map.update!(:players, &Enum.reverse/1)

    assert :ok = GamesSink.handle_report(replay)

    assert Repo.aggregate(Player, :count) == 2
    assert Repo.aggregate(Game, :count) == 1

    game = Repo.get_by!(Game, source: "discord", external_id: "spellbot:SB12345")

    [first, second] =
      game.id |> Games.get_game!() |> Map.fetch!(:seats) |> Enum.sort_by(& &1.seat)

    assert first.player.discord_id == "222"
    assert first.result == "win"
    assert second.player.discord_id == "111"
    assert second.result == "loss"
  end

  test "/won updates the winner of an already-created game" do
    assert :ok = GamesSink.handle_report(report(["111"]))
    start_supervised!({Tracker, sink: GamesSink})

    assert :ok = Tracker.observe(report([]))
    assert {:ok, _completed} = Tracker.record_winner("SB12345", "222")

    game = Repo.get_by!(Game, source: "discord", external_id: "spellbot:SB12345")
    seats = game.id |> Games.get_game!() |> Map.fetch!(:seats)

    assert Repo.aggregate(Game, :count) == 1
    assert Enum.find(seats, &(&1.player.discord_id == "222")).result == "win"
    assert Enum.find(seats, &(&1.player.discord_id == "111")).result == "loss"
  end

  test "winnerless reports stay pending and invalid reports return errors" do
    assert :ok = GamesSink.handle_report(report([]))
    assert Repo.aggregate(Game, :count) == 0
    assert Repo.aggregate(Player, :count) == 0

    invalid = Map.update!(report(["999"]), :players, &Enum.take(&1, 1))

    assert {:error, :invalid_player_count} = GamesSink.handle_report(invalid)
    assert Repo.aggregate(Game, :count) == 0

    tracker = start_supervised!({Tracker, sink: GamesSink})
    assert {:error, :invalid_player_count} = Tracker.observe(invalid)
    assert %{reports: %{}} = :sys.get_state(tracker)
  end

  defp report(winner_discord_ids) do
    %GameReport{
      external_id: "spellbot:SB12345",
      source: "discord",
      played_at: ~U[2026-09-19 18:00:00Z],
      guild_id: "333",
      channel_id: "444",
      players: [
        %{
          discord_id: "111",
          display_name: "Aria",
          commander_name: "Alela, Artful Provocateur"
        },
        %{discord_id: "222", display_name: "Bryn", commander_name: nil}
      ],
      winner_discord_ids: winner_discord_ids,
      raw: %{message_id: "555"}
    }
  end
end
