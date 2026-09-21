defmodule TheGathering.Imports.SheetCommit do
  @moduledoc false
  alias TheGathering.Games
  alias TheGathering.Imports.{SheetPreview, SheetReceipt}
  alias TheGathering.Repo

  def run(params, revision, user_id) do
    Repo.transaction(fn ->
      with {:ok, preview} <- SheetPreview.run(params),
           true <- preview.revision == revision,
           true <- preview.valid do
        Enum.reduce(
          preview.rows,
          %{created: 0, updated: 0, skipped: 0, game_ids: []},
          &commit_row(&1, &2, user_id)
        )
      else
        _ -> Repo.rollback("Preview is stale or invalid. Preview again before importing.")
      end
    end)
  end

  defp commit_row(%{action: "skip"}, result, _user_id),
    do: %{result | skipped: result.skipped + 1}

  defp commit_row(row, result, user_id) do
    {game, kind} =
      case row.action do
        "create" ->
          attrs = %{
            played_at: DateTime.new!(row.date, ~T[12:00:00]),
            notes: row.notes,
            source: "csv",
            external_id: "sheet:" <> row.key,
            seats:
              row.seats
              |> Enum.with_index(1)
              |> Enum.map(fn {seat, index} -> create_seat(seat, index) end)
          }

          {unwrap!(Games.create_game(attrs, user_id)), :created}

        id ->
          game = Games.get_game!(id)

          seats =
            Enum.map(game.seats, fn existing ->
              seat = Enum.find(row.seats, &(&1.player_id == existing.player_id))

              %{
                id: existing.id,
                player_id: existing.player_id,
                seat: existing.seat,
                deck_id: deck_id(seat, existing.player_id),
                result: seat.result,
                kills: if(is_nil(seat.kills), do: existing.kills, else: seat.kills)
              }
            end)

          attrs = %{seats: seats, notes: if(row.notes == "", do: game.notes, else: row.notes)}
          {unwrap!(Games.update_game(game, attrs)), :updated}
      end

    Repo.insert!(%SheetReceipt{key: row.key, game_id: game.id})
    result |> Map.update!(kind, &(&1 + 1)) |> Map.update!(:game_ids, &[game.id | &1])
  end

  defp create_seat(seat, index) do
    player_id =
      case seat.player_id do
        id when is_integer(id) -> id
        _new -> unwrap!(Games.find_or_create_player_by_name(seat.player)).id
      end

    %{
      player_id: player_id,
      deck_id: deck_id(seat, player_id),
      seat: index,
      result: seat.result,
      kills: seat.kills
    }
  end

  defp deck_id(%{deck_id: "new"} = seat, player_id),
    do: unwrap!(Games.find_or_create_deck(player_id, seat.deck, %{commander_name: seat.deck})).id

  defp deck_id(seat, _player_id), do: seat.deck_id

  defp unwrap!({:ok, value}), do: value
  defp unwrap!({:error, changeset}), do: Repo.rollback(changeset)
end
