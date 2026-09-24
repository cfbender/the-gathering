defmodule TheGathering.WebcamTables.TimerTest do
  use ExUnit.Case, async: true

  alias TheGathering.WebcamTables.Timer

  test "timer transitions account for multiple unequal pauses without resetting" do
    timer = Timer.new()
    assert Timer.update(timer, "resume", 10) == timer
    timer = Timer.update(timer, "start", 1000)
    timer = Timer.update(timer, "pause", 13_000)
    assert Timer.update(timer, "pause", 20_000) == timer
    timer = Timer.update(timer, "resume", 22_000)
    assert timer.paused_ms == 9000
    timer = Timer.update(timer, "pause", 41_000)
    timer = Timer.update(timer, "resume", 46_000)
    assert timer == %{started_at: 1000, paused_at: nil, paused_ms: 14_000}
    assert Timer.update(timer, "start", 50_000) == timer
  end
end
