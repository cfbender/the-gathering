defmodule TheGathering.WebcamTables.Turns do
  @moduledoc """
  Pure turn accounting. Times are measured against the shared game's elapsed
  milliseconds, so pauses freeze a turn without a second set of pause bookkeeping.
  Counts increment when a turn starts, including the first turn of the game.
  """

  def new do
    %{active_player_id: nil, counts: %{}, elapsed_ms: %{}, started_elapsed_ms: 0, revision: 0}
  end

  # A team's first seat is its stable accounting key. Keep the full order when
  # grouping: filtering eliminated seats first would silently change teammates.
  def team(seats, player_id) do
    seats
    |> Enum.chunk_every(2)
    |> Enum.find([], &Enum.any?(&1, fn seat -> seat.player_id == player_id end))
  end

  def turn_id(seats, player_id, "two_headed_giant") do
    case team(seats, player_id) do
      [first | _] -> first.player_id
      [] -> player_id
    end
  end

  def turn_id(_seats, player_id, _mode), do: player_id

  defp units(seats, "two_headed_giant") do
    seats
    |> Enum.chunk_every(2)
    |> Enum.map(fn [first | _] = team ->
      %{first | eliminated: not Enum.any?(team, &eligible?/1)}
      |> Map.put(:departed, false)
    end)
  end

  defp units(seats, _mode), do: seats

  def next_player(seats, active_id, mode \\ "commander") do
    active_id = turn_id(seats, active_id, mode)
    seats = units(seats, mode)
    index = Enum.find_index(seats, &(&1.player_id == active_id))
    offset = if index, do: index + 1, else: 0
    rotated = Enum.drop(seats, offset) ++ Enum.take(seats, offset)

    case Enum.find(rotated, &eligible?/1) do
      nil -> nil
      seat -> seat.player_id
    end
  end

  def pass(turns, seats, elapsed, mode \\ "commander") do
    next = next_player(seats, turns.active_player_id, mode)

    times =
      if turns.active_player_id do
        Map.update(
          turns.elapsed_ms,
          turns.active_player_id,
          elapsed - turns.started_elapsed_ms,
          &(&1 + elapsed - turns.started_elapsed_ms)
        )
      else
        turns.elapsed_ms
      end

    counts = if next, do: Map.update(turns.counts, next, 1, &(&1 + 1)), else: turns.counts

    %{
      turns
      | active_player_id: next,
        counts: counts,
        elapsed_ms: times,
        started_elapsed_ms: elapsed,
        revision: turns.revision + 1
    }
  end

  def reconcile(turns, seats, elapsed, mode \\ "commander") do
    active? =
      Enum.any?(units(seats, mode), &(&1.player_id == turns.active_player_id and eligible?(&1)))

    if active? or (is_nil(turns.active_player_id) and is_nil(next_player(seats, nil, mode))),
      do: turns,
      else: pass(turns, seats, elapsed, mode)
  end

  def adjust(turns, player_id, delta) do
    count = turns.counts |> Map.get(player_id, 0) |> Kernel.+(delta) |> max(0) |> min(999)
    %{turns | counts: Map.put(turns.counts, player_id, count)}
  end

  defp eligible?(seat), do: not seat.eliminated and not Map.get(seat, :departed, false)
end
