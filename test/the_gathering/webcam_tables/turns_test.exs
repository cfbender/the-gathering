defmodule TheGathering.WebcamTables.TurnsTest do
  use ExUnit.Case, async: true

  alias TheGathering.WebcamTables.{Timer, Turns}

  test "starts counts on entry, skips out/departed seats and wraps" do
    seats = [seat(1), seat(2, true), Map.put(seat(3), :departed, true), seat(4)]
    turns = Turns.reconcile(Turns.new(), seats, 0)
    assert turns.active_player_id == 1
    assert turns.counts == %{1 => 1}
    turns = Turns.pass(turns, seats, 12_000)
    assert turns.active_player_id == 4
    assert turns.counts == %{1 => 1, 4 => 1}
    turns = Turns.pass(turns, seats, 31_000)
    assert turns.active_player_id == 1
    assert turns.counts == %{1 => 2, 4 => 1}
    assert turns.elapsed_ms == %{1 => 12_000, 4 => 19_000}
    assert turns.started_elapsed_ms == 31_000
  end

  test "turn times exclude pauses, including passing while paused" do
    seats = [seat(1), seat(2)]
    timer = Timer.update(Timer.new(), "start", 1000)
    turns = Turns.pass(Turns.new(), seats, 0)
    timer = Timer.update(timer, "pause", 13_000)
    turns = Turns.pass(turns, seats, Timer.elapsed(timer, 19_000))
    assert turns.elapsed_ms == %{1 => 12_000}
    timer = Timer.update(timer, "resume", 22_000)
    turns = Turns.pass(turns, seats, Timer.elapsed(timer, 41_000))
    assert turns.elapsed_ms == %{1 => 12_000, 2 => 19_000}
    assert turns.started_elapsed_ms == 31_000
    assert turns.counts == %{1 => 2, 2 => 1}
  end

  test "elimination advances once; no survivors clears active turn, restoration starts it" do
    seats = [seat(1), seat(2)]
    turns = Turns.pass(Turns.new(), seats, 0)
    turns = Turns.reconcile(turns, [seat(1, true), seat(2)], 7000)
    assert turns.active_player_id == 2
    assert turns.elapsed_ms == %{1 => 7000}
    assert Turns.reconcile(turns, [seat(1, true), seat(2)], 9000) == turns
    turns = Turns.reconcile(turns, [seat(1, true), seat(2, true)], 11_000)
    assert turns.active_player_id == nil
    assert turns.elapsed_ms == %{1 => 7000, 2 => 4000}
    assert Turns.reconcile(turns, [seat(1, true), seat(2, true)], 15_000) == turns
    turns = Turns.reconcile(turns, [seat(1), seat(2, true)], 20_000)
    assert turns.active_player_id == 1
    assert turns.counts == %{1 => 2, 2 => 1}
    assert turns.started_elapsed_ms == 20_000
  end

  test "corrections clamp at zero and 999 without changing turn timing" do
    turns = Turns.pass(Turns.new(), [seat(1)], 0)
    adjusted = turns |> Turns.adjust(1, -1) |> Turns.adjust(1, -1)
    assert adjusted == %{turns | counts: %{1 => 0}}
    assert Turns.adjust(%{turns | counts: %{1 => 999}}, 1, 1).counts == %{1 => 999}
  end

  test "2HG groups before filtering, accounts once per team and skips eliminated teams" do
    mode = "two_headed_giant"
    seats = [seat(8), seat(3), seat(17, true), seat(5, true), seat(2), seat(11)]
    turns = Turns.reconcile(Turns.new(), seats, 0, mode)
    assert turns.active_player_id == 8
    assert Turns.next_player(seats, 3, mode) == 2
    turns = Turns.pass(turns, seats, 7000, mode)
    assert turns.counts == %{8 => 1, 2 => 1}
    assert turns.elapsed_ms == %{8 => 7000}
    turns = Turns.pass(turns, seats, 19_000, mode)
    assert turns.active_player_id == 8
    assert turns.elapsed_ms == %{8 => 7000, 2 => 12_000}
    assert Turns.turn_id(seats, 11, mode) == 2

    assert Turns.adjust(turns, Turns.turn_id(seats, 3, mode), 1).counts ==
             %{8 => 3, 2 => 1}

    all_out = Enum.map(seats, &%{&1 | eliminated: true})
    assert Turns.reconcile(turns, all_out, 21_000, mode).active_player_id == nil
  end

  defp seat(id, eliminated \\ false), do: %{player_id: id, eliminated: eliminated}
end
