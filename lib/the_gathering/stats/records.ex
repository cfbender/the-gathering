defmodule TheGathering.Stats.Records do
  @moduledoc """
  Win/loss/draw arithmetic shared by every statistics view.

  Rows are `GamePlayer` seats (anything with a `result` of `"win"`, `"loss"`, or
  `"draw"`); games are `Game` structs with preloaded seats, newest first.
  """

  import Ecto.Query

  @doc "Groups seats by `key_fun` and returns one record per group, most played first."
  def grouped_records(rows, entity_fun, key_fun) do
    rows
    |> Enum.group_by(key_fun)
    |> Enum.map(fn {_key, group} -> Map.merge(entity(entity_fun.(hd(group))), record(group)) end)
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
  A compact game summary; `tracked` is the seat (or seats) whose result the caller
  follows, if any. See `tracked_result/1` for how several seats combine.
  """
  def recent_game(game, tracked \\ nil) do
    winner = Enum.find(game.seats, &(&1.result == "win"))

    %{
      id: game.id,
      played_at: game.played_at,
      duration_minutes: game.duration_minutes,
      turns: game.turns,
      result: tracked_result(tracked),
      winner: winner && entity(winner.player),
      players: length(game.seats)
    }
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

  @doc """
  Applies optional inclusive `date_from` / `date_to` ISO dates from `params` to a query
  whose game binding is named `:game`. Unparseable values are ignored.
  """
  def date_range(query, params) do
    query
    |> maybe_date_from(params_value(params, :date_from))
    |> maybe_date_to(params_value(params, :date_to))
  end

  defp params_value(params, key), do: Map.get(params, key) || Map.get(params, Atom.to_string(key))

  defp maybe_date_from(query, nil), do: query

  defp maybe_date_from(query, value) do
    case Date.from_iso8601(value) do
      {:ok, date} -> where(query, [game: game], game.played_at >= ^start_of_day(date))
      _error -> query
    end
  end

  defp maybe_date_to(query, nil), do: query

  defp maybe_date_to(query, value) do
    case Date.from_iso8601(value) do
      {:ok, date} -> where(query, [game: game], game.played_at < ^start_of_day(Date.add(date, 1)))
      _error -> query
    end
  end

  defp start_of_day(date), do: DateTime.new!(date, ~T[00:00:00], "Etc/UTC")

  def average(rows, fun) do
    values = rows |> Enum.map(fun) |> Enum.reject(&is_nil/1)
    if values == [], do: nil, else: Float.round(Enum.sum(values) / length(values), 1)
  end

  def percentage(_part, 0), do: 0.0
  def percentage(part, total), do: Float.round(part * 100 / total, 1)

  def entity(%{id: id, name: name} = value),
    do:
      %{id: id, name: name}
      |> maybe_put(:commander_name, Map.get(value, :commander_name))
      |> maybe_put(:art_crop_url, Map.get(value, :art_crop_url))
      |> maybe_put(:color_identity, Map.get(value, :color_identity))

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
