defmodule TheGathering.Stats.Elo do
  @moduledoc """
  Multiplayer Elo ratings replayed over a list of games.

  Every player starts at #{1000}. After each game, each seat is compared with every
  other seat at the table: the winner scores 1 against each loser, two drawing seats
  score 0.5 against each other, and two losers are not compared (they neither beat nor
  lost to each other). The pairwise differences between actual and expected scores are
  averaged over the seat's opponents and scaled by K = 32, so a game moves at most K
  points in total and the table's rating changes sum to zero.
  """

  @start 1000.0
  @k 32

  @doc """
  Ratings after replaying `games` (newest first, seats preloaded with `:player`),
  highest rating first. Each entry carries the player's current and peak rating, the
  rating they `start`ed the window with, games rated, and a `history` of
  `%{date, rating}` points, oldest first.

  `window_start` is `nil` or `{first_date, starts_at}` from `Query.window_start/1`.
  Games before `starts_at` are replayed so ratings carry in, but only players who
  played inside the window are returned, and their peak, games, and history cover the
  window alone. A carried-in rating opens the history with a point on `first_date`.
  """
  def ratings(games, window_start \\ nil) do
    {before, within} = split_window(Enum.reverse(games), window_start)

    before
    |> Enum.reduce(%{}, &rate_game/2)
    |> Map.new(fn {id, state} -> {id, open_window(state, window_start)} end)
    |> then(&Enum.reduce(within, &1, fn game, states -> rate_game(game, states) end))
    |> Enum.filter(fn {_id, state} -> state.games > 0 end)
    |> Enum.map(fn {_id, state} ->
      %{
        id: state.player.id,
        name: state.player.name,
        rating: round(state.rating),
        start: round(state.start),
        peak: round(state.peak),
        games: state.games,
        history: Enum.reverse(state.history)
      }
    end)
    |> Enum.sort_by(&{-&1.rating, -&1.games, String.downcase(&1.name)})
  end

  defp split_window(games, nil), do: {[], games}

  defp split_window(games, {_date, starts_at}),
    do: Enum.split_while(games, &(DateTime.compare(&1.played_at, starts_at) == :lt))

  defp open_window(state, {date, _starts_at}) do
    %{
      state
      | start: state.rating,
        peak: state.rating,
        games: 0,
        history: [point(Date.to_iso8601(date), state.rating)]
    }
  end

  defp rate_game(game, states) do
    seats = Enum.uniq_by(game.seats, & &1.player_id)
    opponents = length(seats) - 1
    ratings = Map.new(seats, &{&1.player_id, current_rating(states, &1.player_id)})
    date = Date.to_iso8601(DateTime.to_date(game.played_at))

    Enum.reduce(seats, states, fn seat, states ->
      change =
        seats
        |> Enum.reject(&(&1.player_id == seat.player_id))
        |> Enum.map(&pair_change(seat, &1, ratings))
        |> Enum.sum()

      rating = Map.fetch!(ratings, seat.player_id) + @k * change / max(opponents, 1)

      Map.update(
        states,
        seat.player_id,
        new_state(seat.player, rating, date),
        &advance(&1, rating, date)
      )
    end)
  end

  defp pair_change(seat, opponent, ratings) do
    case score(seat.result, opponent.result) do
      nil -> 0.0
      score -> score - expected(ratings[seat.player_id], ratings[opponent.player_id])
    end
  end

  defp score("win", "win"), do: 0.5
  defp score("win", _), do: 1.0
  defp score(_, "win"), do: 0.0
  defp score("draw", "draw"), do: 0.5
  defp score(_, _), do: nil

  defp expected(rating, opponent_rating),
    do: 1 / (1 + :math.pow(10, (opponent_rating - rating) / 400))

  defp current_rating(states, player_id) do
    case states do
      %{^player_id => %{rating: rating}} -> rating
      _ -> @start
    end
  end

  defp new_state(player, rating, date) do
    %{
      player: player,
      rating: rating,
      start: @start,
      peak: max(rating, @start),
      games: 1,
      history: [point(date, rating)]
    }
  end

  defp advance(state, rating, date) do
    %{
      state
      | rating: rating,
        peak: max(state.peak, rating),
        games: state.games + 1,
        history: [point(date, rating) | state.history]
    }
  end

  defp point(date, rating), do: %{date: date, rating: round(rating)}
end
