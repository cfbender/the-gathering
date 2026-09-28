defmodule TheGathering.WebcamTables.TimerTest do
  use ExUnit.Case, async: true

  alias TheGathering.WebcamTables.Timer

  test "timer transitions account for multiple unequal pauses without resetting" do
    timer = Timer.new()
    assert Timer.update(timer, "resume", 10) == timer
    refute Timer.awaiting_start?(timer)
    timer = Timer.update(timer, "start", 1000)
    assert timer == %{started_at: 1000, paused_at: 1000, paused_ms: 0}
    assert Timer.awaiting_start?(timer)
    assert Timer.elapsed(timer, 5000) == 0
    assert Timer.update(timer, "pause", 5000) == timer
    timer = Timer.update(timer, "resume", 5000)
    refute Timer.awaiting_start?(timer)
    assert Timer.elapsed(timer, 5000) == 0
    timer = Timer.update(timer, "pause", 13_000)
    assert Timer.update(timer, "pause", 20_000) == timer
    timer = Timer.update(timer, "resume", 22_000)
    assert timer.paused_ms == 13_000
    timer = Timer.update(timer, "pause", 41_000)
    timer = Timer.update(timer, "resume", 46_000)
    assert timer == %{started_at: 1000, paused_at: nil, paused_ms: 18_000}
    assert Timer.update(timer, "start", 50_000) == timer
  end
end
