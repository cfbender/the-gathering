defmodule TheGatheringWeb.WebcamTableTurnsTest do
  use ExUnit.Case, async: true

  alias TheGatheringWeb.{WebcamTableState, WebcamTableTurns}

  test "starts counts on entry, skips out/departed seats and wraps" do
    seats = [seat(1), seat(2, true), Map.put(seat(3), :departed, true), seat(4)]
    turns = WebcamTableTurns.reconcile(WebcamTableTurns.new(), seats, 0)
    assert turns.active_player_id == 1
    assert turns.counts == %{1 => 1}
    turns = WebcamTableTurns.pass(turns, seats, 12_000)
    assert turns.active_player_id == 4
    assert turns.counts == %{1 => 1, 4 => 1}
    turns = WebcamTableTurns.pass(turns, seats, 31_000)
    assert turns.active_player_id == 1
    assert turns.counts == %{1 => 2, 4 => 1}
    assert turns.elapsed_ms == %{1 => 12_000, 4 => 19_000}
    assert turns.started_elapsed_ms == 31_000
  end

  test "turn times exclude pauses, including passing while paused" do
    seats = [seat(1), seat(2)]
    timer = WebcamTableState.update_timer(WebcamTableState.new_timer(), "start", 1000)
    turns = WebcamTableTurns.pass(WebcamTableTurns.new(), seats, 0)
    timer = WebcamTableState.update_timer(timer, "pause", 13_000)
    turns = WebcamTableTurns.pass(turns, seats, WebcamTableState.elapsed(timer, 19_000))
    assert turns.elapsed_ms == %{1 => 12_000}
    timer = WebcamTableState.update_timer(timer, "resume", 22_000)
    turns = WebcamTableTurns.pass(turns, seats, WebcamTableState.elapsed(timer, 41_000))
    assert turns.elapsed_ms == %{1 => 12_000, 2 => 19_000}
    assert turns.started_elapsed_ms == 31_000
    assert turns.counts == %{1 => 2, 2 => 1}
  end

  test "elimination advances once; no survivors clears active turn, restoration starts it" do
    seats = [seat(1), seat(2)]
    turns = WebcamTableTurns.pass(WebcamTableTurns.new(), seats, 0)
    turns = WebcamTableTurns.reconcile(turns, [seat(1, true), seat(2)], 7000)
    assert turns.active_player_id == 2
    assert turns.elapsed_ms == %{1 => 7000}
    assert WebcamTableTurns.reconcile(turns, [seat(1, true), seat(2)], 9000) == turns
    turns = WebcamTableTurns.reconcile(turns, [seat(1, true), seat(2, true)], 11_000)
    assert turns.active_player_id == nil
    assert turns.elapsed_ms == %{1 => 7000, 2 => 4000}
    assert WebcamTableTurns.reconcile(turns, [seat(1, true), seat(2, true)], 15_000) == turns
    turns = WebcamTableTurns.reconcile(turns, [seat(1), seat(2, true)], 20_000)
    assert turns.active_player_id == 1
    assert turns.counts == %{1 => 2, 2 => 1}
    assert turns.started_elapsed_ms == 20_000
  end

  test "corrections clamp at zero and 999 without changing turn timing" do
    turns = WebcamTableTurns.pass(WebcamTableTurns.new(), [seat(1)], 0)
    adjusted = turns |> WebcamTableTurns.adjust(1, -1) |> WebcamTableTurns.adjust(1, -1)
    assert adjusted == %{turns | counts: %{1 => 0}}
    assert WebcamTableTurns.adjust(%{turns | counts: %{1 => 999}}, 1, 1).counts == %{1 => 999}
  end

  defp seat(id, eliminated \\ false), do: %{player_id: id, eliminated: eliminated}
end
