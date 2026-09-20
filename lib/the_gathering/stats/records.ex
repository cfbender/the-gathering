defmodule TheGathering.Stats.Records do
  @moduledoc """
  Win/loss/draw arithmetic shared by every statistics view.

  Rows are `GamePlayer` seats (anything with a `result` of `"win"`, `"loss"`, or
  `"draw"`); games are `Game` structs with preloaded seats, newest first.
  """

  alias TheGathering.Games.ColorIdentity

  @doc "Groups seats by `key_fun` and returns one record per group, most played first."
  def grouped_records(rows, entity_fun, key_fun) do
    rows
    |> Enum.group_by(key_fun)
    |> Enum.map(fn {_key, group} -> Map.merge(entity_fun.(hd(group)), record(group)) end)
    |> Enum.sort_by(&{-&1.games, -&1.win_rate, String.downcase(&1.name)})
  end

  @doc """
  One record per deck color identity (`id` is the canonical WUBRG letters, `name`
  the guild/shard name). Seats without a deck have no colors and are skipped.
  """
  def color_records(seats) do
    seats
    |> Enum.reject(&is_nil(&1.deck))
    |> grouped_records(
      &%{
        id: ColorIdentity.canonical(&1.deck.color_identity),
        name: ColorIdentity.name(&1.deck.color_identity)
      },
      &ColorIdentity.canonical(&1.deck.color_identity)
    )
  end

  @color_names %{"W" => "White", "U" => "Blue", "B" => "Black", "R" => "Red", "G" => "Green"}

  @doc """
  One record per WUBRG color counting every seat whose deck identity includes that
  color (a Bant deck counts for white, blue, and green), always in WUBRG order.
  `share` is the percentage of deck-bearing seats that ran the color.
  """
  def color_exposure(seats) do
    with_decks = Enum.reject(seats, &is_nil(&1.deck))

    for color <- ~w(W U B R G) do
      rows =
        Enum.filter(
          with_decks,
          &String.contains?(ColorIdentity.canonical(&1.deck.color_identity), color)
        )

      %{id: color, name: @color_names[color], share: percentage(length(rows), length(with_decks))}
      |> Map.merge(record(rows))
    end
  end

  @doc """
  Every ordered pair of players who shared a table: the first player's record in the
  games both sat in. Sorted by most shared games, then the players' names.
  """
  def matchups(games) do
    games
    |> Enum.flat_map(fn game ->
      seats = Enum.uniq_by(game.seats, & &1.player_id)

      for seat <- seats, opponent <- seats, seat.player_id != opponent.player_id do
        {seat, opponent}
      end
    end)
    |> Enum.group_by(fn {seat, opponent} -> {seat.player_id, opponent.player_id} end)
    |> Enum.map(fn {_key, pairs} ->
      {seat, opponent} = hd(pairs)

      %{id: seat.player_id, name: seat.player.name, opponent_id: opponent.player_id}
      |> Map.merge(record(Enum.map(pairs, &elem(&1, 0))))
    end)
    |> Enum.sort_by(&{-&1.games, String.downcase(&1.name), &1.opponent_id})
  end

  @doc """
  Counts `values` (nil skipped) into consecutive `bin_size`-wide bins from the lowest
  value's bin to the highest, including empty bins between them. Each bin is
  `%{from, to, games}` where `to` is exclusive.
  """
  def histogram(values, bin_size) when bin_size > 0 do
    case Enum.reject(values, &is_nil/1) do
      [] ->
        []

      values ->
        first = div(Enum.min(values), bin_size)
        last = div(Enum.max(values), bin_size)
        counts = Enum.frequencies_by(values, &div(&1, bin_size))

        for bin <- first..last do
          %{from: bin * bin_size, to: (bin + 1) * bin_size, games: Map.get(counts, bin, 0)}
        end
    end
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
