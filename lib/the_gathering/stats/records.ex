defmodule TheGathering.Stats.Records do
  @moduledoc """
  Win/loss/draw arithmetic shared by every statistics view.

  Rows are `GamePlayer` seats (anything with a `result` of `"win"`, `"loss"`, or
  `"draw"`); games are `Game` structs with preloaded seats, newest first.
  """

  @doc "Groups seats by `key_fun` and returns one record per group, most played first."
  def grouped_records(rows, entity_fun, key_fun) do
    rows
    |> Enum.group_by(key_fun)
    |> Enum.map(fn {_key, group} -> Map.merge(entity_fun.(hd(group)), record(group)) end)
    |> Enum.sort_by(&{-&1.games, -&1.win_rate, String.downcase(&1.name)})
  end

  def record(rows) do
    wins = Enum.count(rows, &(&1.result == "win"))
    losses = Enum.count(rows, &(&1.result == "loss"))
    draws = Enum.count(rows, &(&1.result == "draw"))
    games = length(rows)

    %{games: games, wins: wins, losses: losses, draws: draws, win_rate: percentage(wins, games)}
  end

  @doc """
  Cumulative win rate after each game, oldest first. `seats_fun` picks the tracked
  seats from a game's seats (a seat, a list of seats, or `nil`); games where it picks
  nothing are skipped. Every tracked seat counts as an appearance, so the final point
  always equals `record/1` over the same seats even when several tracked seats share
  one game (a commander mirror match).
  """
  def cumulative_win_rate(games, seats_fun) do
    games
    |> Enum.reverse()
    |> Enum.reduce({[], 0, 0}, fn game, acc ->
      add_trend_point(acc, game, List.wrap(seats_fun.(game.seats)))
    end)
    |> elem(0)
    |> Enum.reverse()
  end

  defp add_trend_point(acc, _game, []), do: acc

  defp add_trend_point({points, wins, total}, game, seats) do
    wins = wins + Enum.count(seats, &(&1.result == "win"))
    total = total + length(seats)

    point = %{
      date: Date.to_iso8601(DateTime.to_date(game.played_at)),
      win_rate: percentage(wins, total)
    }

    {[point | points], wins, total}
  end

  @doc """
  The result to show for the tracked seat(s) of one game: a single seat's own result,
  or, when several tracked seats shared a game, `"win"` if any of them won, `"draw"`
  if any drew, and `"loss"` otherwise.
  """
  def tracked_result(nil), do: nil
  def tracked_result([]), do: nil
  def tracked_result(%{result: result}), do: result

  def tracked_result(seats) when is_list(seats) do
    results = Enum.map(seats, & &1.result)
    Enum.find(["win", "draw"], "loss", &(&1 in results))
  end

  def average(rows, fun) do
    values = rows |> Enum.map(fun) |> Enum.reject(&is_nil/1)
    if values == [], do: nil, else: Float.round(Enum.sum(values) / length(values), 1)
  end

  def percentage(_part, 0), do: 0.0
  def percentage(part, total), do: Float.round(part * 100 / total, 1)
end
