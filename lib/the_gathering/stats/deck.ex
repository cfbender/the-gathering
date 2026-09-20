defmodule TheGathering.Stats.Deck do
  @moduledoc "Calculates one deck's statistics view."

  alias TheGathering.{Accounts, Repo}
  alias TheGathering.Games.Deck
  alias TheGathering.Stats.{Query, Records, Summaries}

  def get(deck_id, params \\ %{}) do
    with %Deck{} = deck <- Repo.get(Deck, deck_id) |> Repo.preload(:player) do
      games = Query.games(params, deck_id: deck.id)
      seats = Enum.map(games, &deck_seat(&1, deck.id))
      cutoff = Accounts.get_settings().detailed_stats_from
      detailed = detailed_games(games, cutoff)

      %{
        detailed_stats_from: cutoff,
        deck: Summaries.entity(deck),
        player: Summaries.entity(deck.player),
        record: Records.record(seats),
        average_duration_minutes: Records.average(detailed, & &1.duration_minutes),
        average_turns: Records.average(detailed, & &1.turns),
        opponents: deck_opponents(games, deck.id),
        recent_games:
          games |> Enum.take(10) |> Enum.map(&Summaries.recent_game(&1, deck_seat(&1, deck.id))),
        win_rate_over_time:
          Records.cumulative_win_rate(
            games,
            &Enum.find(&1, fn seat -> seat.deck_id == deck.id end)
          )
      }
    end
  end

  defp detailed_games(games, nil), do: games

  defp detailed_games(games, %Date{} = cutoff) do
    Enum.filter(games, &(Date.compare(DateTime.to_date(&1.played_at), cutoff) != :lt))
  end

  defp deck_seat(game, deck_id), do: Enum.find(game.seats, &(&1.deck_id == deck_id))

  defp deck_opponents(games, deck_id) do
    games
    |> Enum.flat_map(fn game -> Enum.reject(game.seats, &(&1.deck_id == deck_id)) end)
    |> Enum.reject(&is_nil(&1.player))
    |> Records.grouped_records(&Summaries.entity(&1.player), & &1.player_id)
  end
end
