defmodule TheGathering.Imports.SheetPreview do
  @moduledoc false
  import Ecto.Query

  alias TheGathering.Games
  alias TheGathering.Games.Game
  alias TheGathering.Imports.{GoogleSheet, SheetReceipt, SheetResolution}
  alias TheGathering.Repo

  def run(params) do
    with {:ok, rows} <- GoogleSheet.parse(params["text"]) do
      players =
        Games.list_players(%{include_archived: true}) |> Enum.map(&Map.take(&1, [:id, :name]))

      decks =
        Games.list_decks(%{include_archived: true})
        |> Enum.map(&Map.take(&1, [:id, :player_id, :name, :commander_name]))

      keys = Enum.map(rows, & &1.key)
      receipts = Repo.all(from receipt in SheetReceipt, where: receipt.key in ^keys)
      imported = Map.new(receipts, &{&1.key, &1.game_id})
      candidates = candidates(rows)

      rows =
        Enum.map(rows, fn row ->
          nearby = Enum.filter(candidates, &nearby?(&1, row.date))
          SheetResolution.resolve(row, params, players, decks, nearby, imported[row.key])
        end)

      rows = reject_duplicate_targets(rows, params)
      selected = Enum.reject(rows, &(&1.action == "skip"))

      preview = %{
        rows: rows,
        players: players,
        decks: decks,
        valid: selected != [] and Enum.all?(selected, &(&1.errors == []))
      }

      {:ok, Map.put(preview, :revision, fingerprint({params, preview}))}
    end
  end

  defp candidates(rows) do
    dates = rows |> Enum.map(& &1.date) |> Enum.reject(&is_nil/1)

    case dates do
      [] ->
        []

      dates ->
        first = dates |> Enum.min(Date) |> Date.add(-1) |> DateTime.new!(~T[00:00:00])
        last = dates |> Enum.max(Date) |> Date.add(2) |> DateTime.new!(~T[00:00:00])

        Repo.all(
          from game in Game,
            where: game.played_at >= ^first and game.played_at < ^last,
            order_by: [asc: game.played_at, asc: game.id],
            preload: [seats: [:player, :deck]]
        )
        |> Enum.map(&snapshot/1)
    end
  end

  # Include all persisted fields in the revision, not second-resolution updated_at alone.
  defp snapshot(game) do
    game
    |> Map.take([:id, :played_at, :notes, :turns, :duration_minutes, :source, :external_id])
    |> Map.put(
      :seats,
      Enum.map(game.seats, fn seat ->
        seat
        |> Map.take([
          :id,
          :player_id,
          :deck_id,
          :seat,
          :result,
          :kills,
          :notes,
          :mvp_card_id,
          :mvp_card_name,
          :eliminated_turn,
          :eliminated_by_player_id
        ])
        |> Map.merge(%{player: seat.player.name, deck: seat.deck && seat.deck.name})
      end)
    )
  end

  defp nearby?(_game, nil), do: false
  defp nearby?(game, date), do: abs(Date.diff(DateTime.to_date(game.played_at), date)) <= 1

  defp reject_duplicate_targets(rows, params) do
    duplicates = rows |> Enum.filter(&is_integer(&1.action)) |> Enum.frequencies_by(& &1.action)

    Enum.map(rows, fn row ->
      if Map.get(duplicates, row.action, 0) > 1 do
        %{
          row
          | errors:
              row.errors ++ ["Two sheet rows target the same game. Choose which one to use."],
            status: "review",
            action: if(get_in(params, ["actions", row.key]), do: row.action, else: "skip")
        }
      else
        row
      end
    end)
  end

  defp fingerprint(value),
    do: :crypto.hash(:sha256, :erlang.term_to_binary(value)) |> Base.encode16(case: :lower)
end
