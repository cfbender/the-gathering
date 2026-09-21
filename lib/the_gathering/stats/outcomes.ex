defmodule TheGathering.Stats.Outcomes do
  @moduledoc "Kill totals and recorded win conditions, independent of the detailed-stats cutoff."

  def kills(seats) do
    recorded = Enum.reject(seats, &is_nil(&1.kills))

    players =
      recorded
      |> Enum.group_by(& &1.player_id)
      |> Enum.map(fn {id, rows} ->
        total = Enum.sum(Enum.map(rows, & &1.kills))

        %{
          id: id,
          name: hd(rows).player.name,
          kills: total,
          recorded_games: length(rows),
          average: Float.round(total / length(rows), 2)
        }
      end)
      |> Enum.sort_by(&{-&1.kills, String.downcase(&1.name), &1.id})

    %{
      total: Enum.sum(Enum.map(recorded, & &1.kills)),
      recorded_seats: length(recorded),
      total_seats: length(seats),
      players: players
    }
  end

  def win_conditions(games) do
    recorded = Enum.reject(games, &(&1.win_condition in [nil, "unknown"]))

    conditions =
      recorded
      |> Enum.frequencies_by(& &1.win_condition)
      |> Enum.map(fn {condition, count} -> %{condition: condition, games: count} end)
      |> Enum.sort_by(&{-&1.games, &1.condition})

    %{recorded_games: length(recorded), total_games: length(games), conditions: conditions}
  end
end
