defmodule TheGathering.Stats.EloTest do
  use ExUnit.Case, async: true

  alias TheGathering.Stats.Elo

  @alice %{id: 1, name: "Alice"}
  @bob %{id: 2, name: "Bob"}
  @cara %{id: 3, name: "Cara"}

  # `results` maps players to results; games are given oldest first here and reversed
  # into the newest-first order the stats views load.
  defp games(rows) do
    rows
    |> Enum.with_index(1)
    |> Enum.map(fn {{date, results}, id} ->
      %{
        id: id,
        played_at: DateTime.new!(date, ~T[20:00:00Z]),
        seats:
          Enum.map(results, fn {player, result} ->
            %{player_id: player.id, player: player, result: result}
          end)
      }
    end)
    |> Enum.reverse()
  end

  test "a win at equal ratings moves the table by K in total and sums to zero" do
    ratings =
      Elo.ratings(games([{~D[2026-01-01], [{@alice, "win"}, {@bob, "loss"}, {@cara, "loss"}]}]))

    assert Enum.map(ratings, &{&1.name, &1.rating, &1.peak, &1.games}) == [
             {"Alice", 1016, 1016, 1},
             {"Bob", 992, 1000, 1},
             {"Cara", 992, 1000, 1}
           ]

    assert Enum.map(ratings, & &1.history) == [
             [%{date: "2026-01-01", rating: 1016}],
             [%{date: "2026-01-01", rating: 992}],
             [%{date: "2026-01-01", rating: 992}]
           ]
  end

  test "an underdog gains more than the favourite would have, and losers are not compared" do
    ratings =
      Elo.ratings(
        games([
          {~D[2026-01-01], [{@alice, "win"}, {@bob, "loss"}, {@cara, "loss"}]},
          {~D[2026-01-15], [{@bob, "win"}, {@alice, "loss"}, {@cara, "loss"}]}
        ])
      )

    # Bob (992) beating Alice (1016) is worth 0.534 of a point against her plus 0.5
    # against Cara: +16.55 → 1009. Alice only loses her half of that pair (−8.55 → 1007);
    # Cara, a loser both times, is never compared with Alice and drops 8 → 984.
    assert Enum.map(ratings, &{&1.name, &1.rating, &1.peak}) == [
             {"Bob", 1009, 1009},
             {"Alice", 1007, 1016},
             {"Cara", 984, 1000}
           ]

    assert Enum.find(ratings, &(&1.name == "Alice")).history == [
             %{date: "2026-01-01", rating: 1016},
             %{date: "2026-01-15", rating: 1007}
           ]
  end

  test "a drawn game between equal players changes nothing, an unequal draw favours the underdog" do
    equal = Elo.ratings(games([{~D[2026-01-01], [{@alice, "draw"}, {@bob, "draw"}]}]))
    assert Enum.map(equal, & &1.rating) == [1000, 1000]

    unequal =
      Elo.ratings(
        games([
          {~D[2026-01-01], [{@alice, "win"}, {@bob, "loss"}]},
          {~D[2026-01-02], [{@alice, "draw"}, {@bob, "draw"}]}
        ])
      )

    # 1016 vs 984: Alice expected 0.546, scored 0.5 → −1.47; Bob mirrors it.
    assert Enum.map(unequal, &{&1.name, &1.rating}) == [{"Alice", 1015}, {"Bob", 985}]
  end

  test "ratings replay oldest first regardless of the list order given" do
    rows = [
      {~D[2026-01-01], [{@alice, "win"}, {@bob, "loss"}]},
      {~D[2026-01-02], [{@bob, "win"}, {@alice, "loss"}]}
    ]

    newest_first = Elo.ratings(games(rows))
    # Bob wins the later game as the underdog, so he ends ahead; if the order were
    # replayed backwards Alice would.
    assert Enum.map(newest_first, &{&1.name, &1.rating}) == [{"Bob", 1001}, {"Alice", 999}]
  end
end
