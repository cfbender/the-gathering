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

  defp life(from, to, actor \\ "a", name \\ "Alice"),
    do: %{
      text: "#{name}: #{from} → #{to} life",
      actor: actor,
      kind: "life",
      life: %{name: name, from: from, to: to}
    }

  defp texts(contents), do: Enum.map(contents, & &1.text)

  test "coalesces rapid life changes from the original total through the final total" do
    log =
      [] |> Log.append(life(40, 39), 1000) |> Log.append(life(39, 38), 1500)

    log = Log.append(log, life(38, 37), 2000)

    assert [%{id: 1, text: "Alice: 40 → 37 life", count: 3, at: 2000}] = log
  end

  test "merges at the window boundary but not beyond it, backwards in time, or across table events" do
    first = Log.append([], life(40, 39), 1000)
    assert length(Log.append(first, life(39, 35), 6000)) == 1
    assert length(Log.append(first, life(39, 35), 6001)) == 2
    assert length(Log.append(first, life(39, 35), 999)) == 2

    log = first |> Log.append(%{text: "Seat order randomized"}, 1100)
    assert Enum.map(Log.append(log, life(39, 37), 1200), & &1.id) == [3, 2, 1]
  end

  test "keeps one line per player when several change life at once" do
    log =
      [
        {life(40, 37, "a"), 1000},
        {life(40, 37, "b", "Bob"), 1100},
        {life(40, 37, "c", "Cara"), 1200},
        {life(37, 34, "a"), 1300},
        {life(37, 34, "b", "Bob"), 1400},
        {life(34, 31, "a"), 1500},
        {life(37, 34, "c", "Cara"), 1600}
      ]
      |> Enum.reduce([], fn {content, at}, log -> Log.append(log, content, at) end)

    assert [
             %{id: 3, text: "Cara: 40 → 34 life", count: 2, at: 1600},
             %{id: 2, text: "Bob: 40 → 34 life", count: 2},
             %{id: 1, text: "Alice: 40 → 31 life", count: 3, at: 1500}
           ] = log

    # The window runs from each player's latest change.
    assert [%{id: 3}, %{id: 2}, %{id: 1, text: "Alice: 40 → 28 life", count: 4}] =
             Log.append(log, life(31, 28), 6500)
  end

  test "coalesces counters per player alongside the life changes they come with" do
    hit = fn from, to ->
      Log.seat_changes(
        seat(%{life: 40 - from, commander_damage: %{"2" => %{"Kangee" => from}}}),
        seat(%{life: 40 - to, commander_damage: %{"2" => %{"Kangee" => to}}}),
        [seat(%{player_id: 2, player_name: "Bob", peer_id: "b"})]
      )
    end

    log =
      [hit.(0, 1), hit.(1, 2), hit.(2, 3)]
      |> Enum.with_index()
      |> Enum.reduce([], fn {contents, index}, log ->
        Enum.reduce(contents, log, &Log.append(&2, &1, 1000 + index * 100))
      end)

    assert [
             %{text: "Alice damage from Bob's Kangee: 0 → 3", count: 3},
             %{text: "Alice: 40 → 37 life", count: 3}
           ] = log
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

  test "logs shared custom counters by id, ignoring renames, removals and combat buffs" do
    lands = %{"id" => "c1", "label" => "Lands", "value" => 6}
    storm = %{"id" => "c2", "label" => "Storm", "value" => 3}
    # A seat saved before custom counters existed has no key at all.
    before = seat(%{custom_counters: [lands, storm]})
    legacy = seat()

    after_ =
      seat(%{
        custom_counters: [%{lands | "value" => 7}, %{storm | "label" => "Storm count"}],
        combat_effects: [%{"id" => "e1", "name" => "Anthem"}]
      })

    assert texts(Log.seat_changes(before, after_, [])) == ["Alice Lands: 6 → 7"]

    assert texts(Log.seat_changes(legacy, after_, [])) == [
             "Alice Lands: 0 → 7",
             "Alice Storm count: 0 → 3"
           ]

    assert Log.seat_changes(after_, seat(%{custom_counters: []}), []) == []
    assert Log.seat_changes(after_, legacy, []) == []
  end

  test "names joins, leaves, eliminations, the monarch and seat order" do
    assert Log.joined("Alice").text == "Alice joined the table"
    assert Log.left("Alice").text == "Alice left the table"
    assert Log.elimination(seat(), true).text == "Alice was eliminated"
    assert Log.elimination(seat(), false).text == "Alice was restored to the game"
    assert Log.monarch(%{player_name: "Alice"}).text == "Alice took the monarch"
    alice = %{peer_id: "a", player_name: "Alice"}
    assert Log.monarch(alice, alice).text == "Alice took the monarch"

    assert Log.monarch(alice, %{peer_id: "b", player_name: "Bob"}).text ==
             "Bob gave Alice the monarch"

    assert Log.seat_order(true, false).text == "Seat order randomized"
    assert Log.seat_order(false, false).text == "Game started in seat order"
    assert Log.seat_order(false, true).text == "Seat order changed"
  end
end
