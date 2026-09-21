defmodule TheGathering.Imports.PortableExport do
  @moduledoc false
  import Ecto.Query

  alias TheGathering.Catalog
  alias TheGathering.Catalog.Printing
  alias TheGathering.Games.{Deck, Game, Player}
  alias TheGathering.Imports.{PortableFile, SheetReceipt}
  alias TheGathering.Repo

  def run do
    Repo.transaction(fn ->
      players = Repo.all(from p in Player, order_by: p.id)
      decks = Repo.all(from d in Deck, order_by: d.id)
      games = Repo.all(from g in Game, order_by: g.id, preload: [:seats])
      game_keys = Map.new(games, &{&1.id, &1.portable_id})

      %{
        format: "the-gathering",
        version: 1,
        exported_at: DateTime.utc_now() |> DateTime.truncate(:second),
        players: Enum.map(players, &Map.take(&1, [:id | PortableFile.fields(:players)])),
        decks: Enum.map(decks, &Map.take(&1, [:id, :player_id | PortableFile.fields(:decks)])),
        games: Enum.map(games, &game/1),
        cards: cards(decks, games),
        printings: printings(decks),
        sheet_receipts:
          Enum.map(
            Repo.all(SheetReceipt),
            &%{key: &1.key, game_portable_id: game_keys[&1.game_id]}
          )
      }
    end)
  end

  defp game(game) do
    game
    |> Map.take(PortableFile.fields(:games))
    |> Map.put(
      :seats,
      game.seats
      |> Enum.sort_by(& &1.seat)
      |> Enum.map(&Map.take(&1, PortableFile.fields(:seats)))
    )
  end

  defp cards(decks, games) do
    references =
      Enum.flat_map(
        decks,
        &[{&1.commander_card_id, &1.commander_name}, {&1.partner_card_id, &1.partner_name}]
      ) ++
        Enum.flat_map(games, fn game ->
          Enum.map(game.seats, &{&1.mvp_card_id, &1.mvp_card_name})
        end)

    references
    |> Enum.uniq()
    |> Enum.map(fn {id, name} -> Catalog.resolve_card(id, name) end)
    |> Enum.reject(&is_nil/1)
    |> Enum.uniq_by(& &1.id)
    |> Enum.sort_by(& &1.id)
    |> Enum.map(&Map.take(&1, PortableFile.fields(:cards)))
  end

  defp printings(decks) do
    ids =
      Enum.flat_map(decks, &[&1.commander_printing_id, &1.partner_printing_id])
      |> Enum.reject(&is_nil/1)

    Repo.all(from p in Printing, where: p.id in ^ids, order_by: p.id)
    |> Enum.map(&Map.take(&1, PortableFile.fields(:printings)))
  end
end
