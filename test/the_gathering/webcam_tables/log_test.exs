defmodule TheGathering.WebcamTables.LogTest do
  use ExUnit.Case, async: true

  alias TheGathering.WebcamTables.Log

  defp seat(overrides \\ %{}) do
    Map.merge(
      %{
        peer_id: "a",
        player_id: 1,
        player_name: "Alice",
        life: 40,
        camera_off: false,
        poison: 0,
        rad: 0,
        commander_casts: %{},
        commander_damage: %{},
        eliminated: false
      },
      overrides
    )
  end

  defp life(from, to, actor \\ "a"),
    do: %{
      text: "Alice: #{from} → #{to} life",
      actor: actor,
      kind: "life",
      life: %{name: "Alice", from: from, to: to}
    }

  defp texts(contents), do: Enum.map(contents, & &1.text)

  test "coalesces rapid life changes from the original total through the final total" do
    log =
      [] |> Log.append(life(40, 39), 1000) |> Log.append(life(39, 38), 1500)

    log = Log.append(log, life(38, 37), 2000)

    assert [%{id: 1, text: "Alice: 40 → 37 life", count: 3, at: 2000}] = log
  end

  test "merges at the window boundary but not beyond it, backwards in time, or across events" do
    first = Log.append([], life(40, 39), 1000)
    assert length(Log.append(first, life(39, 35), 3000)) == 1
    assert length(Log.append(first, life(39, 35), 3001)) == 2
    assert length(Log.append(first, life(39, 35), 999)) == 2

    for between <- [life(40, 38, "b"), %{text: "Seat order randomized"}] do
      log = first |> Log.append(between, 1100) |> Log.append(life(39, 37), 1200)
      assert Enum.map(log, & &1.id) == [3, 2, 1]
    end
  end

  test "preserves every roll result and caps the history" do
    roll = fn result ->
      Log.roll(%{kind: "dice", sides: 20, result: result, actor: "a", player_name: "Alice"})
    end

    assert [%{text: "Alice rolled a d20: 17, 3", count: 2}] =
             [] |> Log.append(roll.(17), 1000) |> Log.append(roll.(3), 1100)

    log = Enum.reduce(1..250, [], &Log.append(&2, %{text: "line #{&1}"}, &1))
    assert length(log) == 200
    assert hd(log) == %{id: 250, at: 250, text: "line 250"}
  end

  test "describes every changed seat fact and nothing else" do
    before = seat()

    after_ =
      seat(%{
        life: 37,
        deck_id: 4,
        deck_name: "Birds",
        camera_off: true,
        poison: 10,
        commander_casts: %{"Tymna" => 2},
        commander_damage: %{"2" => %{"Kangee" => 21}}
      })

    seats = [after_, seat(%{player_id: 2, player_name: "Bob", peer_id: "b"})]

    assert texts(Log.seat_changes(before, after_, seats)) == [
             "Alice chose Birds",
             "Alice: 40 → 37 life",
             "Alice turned their camera off",
             "Alice poison: 0 → 10",
             "Alice Tymna commander tax: 0 → 4",
             "Alice damage from Bob's Kangee: 0 → 21"
           ]

    assert Log.seat_changes(after_, after_, seats) == []

    assert "Alice damage from player 9's Kangee: 21 → 0" in texts(
             Log.seat_changes(seat(%{commander_damage: %{"9" => %{"Kangee" => 21}}}), before, [])
           )
  end

  test "names joins, leaves, eliminations, the monarch and seat order" do
    assert Log.joined("Alice").text == "Alice joined the table"
    assert Log.left("Alice").text == "Alice left the table"
    assert Log.elimination(seat(), true).text == "Alice was eliminated"
    assert Log.elimination(seat(), false).text == "Alice was restored to the game"
    assert Log.monarch(%{player_name: "Alice"}).text == "Alice took the monarch"
    assert Log.seat_order(true, false).text == "Seat order randomized"
    assert Log.seat_order(false, false).text == "Game started in seat order"
    assert Log.seat_order(false, true).text == "Seat order changed"
  end
end
