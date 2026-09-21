defmodule TheGathering.Stats.Overview do
  @moduledoc "Calculates the playgroup overview statistics view."

  alias TheGathering.{Accounts, Stats}
  alias TheGathering.Stats.{Elo, Outcomes, Query, Records, Summaries}

  def get(params \\ %{}) do
    games = Query.games(params)
    seats = Enum.flat_map(games, & &1.seats)
    cutoff = Accounts.get_settings().detailed_stats_from
    detailed = detailed_games(games, cutoff)
    detailed_seats = Enum.flat_map(detailed, & &1.seats)

    %{
      detailed_stats_from: cutoff,
      games_count: length(games),
      kills: Outcomes.kills(seats),
      win_conditions: Outcomes.win_conditions(games),
      average_duration_minutes: Records.average(detailed, & &1.duration_minutes),
      average_turns: Records.average(detailed, & &1.turns),
      game_lengths: Summaries.game_lengths(detailed),
      game_times: Enum.map(games, & &1.played_at),
      leaderboard: Records.grouped_records(seats, &Summaries.entity(&1.player), & &1.player_id),
      elo: Elo.ratings(games),
      matchups: Records.matchups(games),
      games_by_month:
        games
        |> Enum.group_by(&Calendar.strftime(&1.played_at, "%Y-%m"))
        |> Enum.map(fn {month, rows} -> %{month: month, games: length(rows)} end)
        |> Enum.sort_by(& &1.month),
      seat_win_rates:
        Records.grouped_records(
          detailed_seats,
          &%{id: &1.seat, name: "Seat #{&1.seat}"},
          & &1.seat
        ),
      color_win_rates: Records.color_records(seats),
      color_exposure: Records.color_exposure(seats),
      commanders: params |> Stats.Commanders.list() |> Enum.take(8),
      recent_games: games |> Enum.take(6) |> Summaries.recent_games()
    }
  end

  defp detailed_games(games, nil), do: games

  defp detailed_games(games, %Date{} = cutoff) do
    Enum.filter(games, &(Date.compare(DateTime.to_date(&1.played_at), cutoff) != :lt))
  end
end
