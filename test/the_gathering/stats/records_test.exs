defmodule TheGathering.Stats.RecordsTest do
  use ExUnit.Case, async: true

  alias TheGathering.Stats.Records

  describe "histogram/2" do
    test "bins from the lowest to the highest value with empty bins between" do
      assert Records.histogram([62, 75, 14, nil, 121], 15) == [
               %{from: 0, to: 15, games: 1},
               %{from: 15, to: 30, games: 0},
               %{from: 30, to: 45, games: 0},
               %{from: 45, to: 60, games: 0},
               %{from: 60, to: 75, games: 1},
               %{from: 75, to: 90, games: 1},
               %{from: 90, to: 105, games: 0},
               %{from: 105, to: 120, games: 0},
               %{from: 120, to: 135, games: 1}
             ]
    end

    test "treats the upper edge as exclusive and skips missing values entirely" do
      assert Records.histogram([9, 10, 11], 2) == [
               %{from: 8, to: 10, games: 1},
               %{from: 10, to: 12, games: 2}
             ]

      assert Records.histogram([nil, nil], 2) == []
    end
  end

  describe "matchups/1" do
    test "records each player's results only in games shared with the opponent" do
      alice = %{id: 1, name: "Alice"}
      bob = %{id: 2, name: "Bob"}
      cara = %{id: 3, name: "Cara"}

      games = [
        %{seats: [seat(alice, "win"), seat(bob, "loss"), seat(cara, "loss")]},
        %{seats: [seat(bob, "win"), seat(alice, "loss")]},
        %{seats: [seat(cara, "win"), seat(bob, "loss")]}
      ]

      rows = Records.matchups(games)

      # Alice never met Cara in the two-player games, so their pair has one game.
      assert Enum.find(rows, &(&1.id == 1 and &1.opponent_id == 3)) ==
               %{
                 id: 1,
                 name: "Alice",
                 opponent_id: 3,
                 games: 1,
                 wins: 1,
                 losses: 0,
                 draws: 0,
                 win_rate: 100.0
               }

      assert %{games: 2, wins: 1, win_rate: 50.0} =
               Enum.find(rows, &(&1.id == 1 and &1.opponent_id == 2))

      assert %{games: 2, wins: 1, win_rate: 50.0} =
               Enum.find(rows, &(&1.id == 2 and &1.opponent_id == 1))

      assert %{games: 2, wins: 0, losses: 2} =
               Enum.find(rows, &(&1.id == 2 and &1.opponent_id == 3))

      assert length(rows) == 6
    end
  end

  describe "color_exposure/1" do
    test "counts a seat once per color in its deck and reports the share of deck-bearing seats" do
      seats = [
        %{result: "win", deck: %{color_identity: "GUW"}},
        %{result: "loss", deck: %{color_identity: "R"}},
        %{result: "loss", deck: %{color_identity: "WU"}},
        %{result: "win", deck: nil}
      ]

      rows = Records.color_exposure(seats)
      assert Enum.map(rows, & &1.id) == ~w(W U B R G)

      assert Enum.map(rows, &{&1.id, &1.games, &1.wins, &1.win_rate, &1.share}) == [
               {"W", 2, 1, 50.0, 66.7},
               {"U", 2, 1, 50.0, 66.7},
               {"B", 0, 0, 0.0, 0.0},
               {"R", 1, 0, 0.0, 33.3},
               {"G", 1, 1, 100.0, 33.3}
             ]

      assert hd(rows).name == "White"
    end
  end

  defp seat(player, result), do: %{player_id: player.id, player: player, result: result}
end
