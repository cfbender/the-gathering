defmodule TheGathering.Discord.StartTimeTest do
  use ExUnit.Case, async: true
  alias TheGathering.Discord.StartTime
  @now ~U[2026-09-23 18:15:00Z]
  @zone "America/New_York"

  test "omitted starts when filled, bare times use today or the next local day" do
    assert {:ok, nil} = StartTime.parse(nil, @now, @zone)
    assert {:ok, ~U[2026-09-24 00:00:00Z]} = StartTime.parse("8pm", @now, @zone)
    assert {:ok, ~U[2026-09-24 00:30:00Z]} = StartTime.parse("20:30", @now, @zone)
    assert {:ok, ~U[2026-09-24 17:30:00Z]} = StartTime.parse("1:30pm", @now, @zone)
    assert {:ok, ~U[2026-09-24 18:15:00Z]} = StartTime.parse("14:15", @now, @zone)
    assert {:ok, ~U[2026-09-24 23:00:00Z]} = StartTime.parse("Tomorrow 7PM", @now, @zone)
  end

  test "noon and midnight are distinct and timezones are configurable" do
    assert {:ok, ~U[2026-09-24 04:00:00Z]} = StartTime.parse("12am", @now, @zone)
    assert {:ok, ~U[2026-09-24 16:00:00Z]} = StartTime.parse("12pm", @now, @zone)
    assert {:ok, ~U[2026-09-23 20:30:00Z]} = StartTime.parse("20:30", @now, "Etc/UTC")
    assert {:error, _} = StartTime.parse("20:30", @now, "Not/AZone")
  end

  test "relative duration is elapsed time, including over DST" do
    assert {:ok, ~U[2026-09-23 19:00:00Z]} = StartTime.parse("in 45m", @now, @zone)
    assert {:ok, ~U[2026-09-23 20:15:00Z]} = StartTime.parse("in 2h", @now, @zone)

    assert {:ok, ~U[2026-03-08 07:15:00Z]} =
             StartTime.parse("in 45m", ~U[2026-03-08 06:30:00Z], @zone)
  end

  test "Discord timestamps are absolute regardless of timezone or display style" do
    unix = DateTime.to_unix(~U[2026-09-23 19:00:00Z])

    for suffix <- ["", ":F", ":R", ":t"] do
      assert {:ok, ~U[2026-09-23 19:00:00Z]} =
               StartTime.parse("<t:#{unix}#{suffix}>", @now, @zone)
    end
  end

  test "rejects past, equal, empty, and malformed inputs" do
    for input <- [
          "<t:1:F>",
          "<t:#{DateTime.to_unix(@now)}>",
          "in 0m",
          "in -1h",
          "25:00",
          "0pm",
          "13pm",
          "8:77am",
          "next week",
          "",
          "<t:99999999999999999999999>"
        ] do
      assert {:error, _} = StartTime.parse(input, @now, @zone)
    end
  end

  test "tomorrow and rollover use calendar days across spring and autumn DST" do
    assert {:ok, ~U[2026-03-08 23:00:00Z]} =
             StartTime.parse("tomorrow 7pm", ~U[2026-03-07 18:00:00Z], @zone)

    assert {:ok, ~U[2026-03-08 17:00:00Z]} =
             StartTime.parse("1pm", ~U[2026-03-07 19:00:00Z], @zone)

    assert {:ok, ~U[2026-11-02 00:00:00Z]} =
             StartTime.parse("tomorrow 7pm", ~U[2026-10-31 18:00:00Z], @zone)
  end

  test "rejects nonexistent and ambiguous DST wall times rather than guessing" do
    assert {:error, gap} = StartTime.parse("tomorrow 2:30am", ~U[2026-03-07 18:00:00Z], @zone)
    assert gap =~ "does not exist"

    assert {:error, ambiguous} =
             StartTime.parse("tomorrow 1:30am", ~U[2026-10-31 18:00:00Z], @zone)

    assert ambiguous =~ "occurs twice"
  end
end
