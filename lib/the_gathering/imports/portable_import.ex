defmodule TheGathering.Imports.PortableImport do
  @moduledoc false
  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Games
  alias TheGathering.Games.{Deck, Game, Player}
  alias TheGathering.Imports.{PortableCatalog, PortableFile, SheetReceipt}
  alias TheGathering.Repo

  # Preview exercises the same constraints as commit, then rolls back every write.
  # No network requests, account linking, or background jobs run during either path.
  def preview(json) do
    with {:ok, data} <- PortableFile.decode(json) do
      result = Repo.transaction(fn -> Repo.rollback({:preview, restore(data, nil)}) end)

      case result do
        {:error, {:preview, result}} -> {:ok, result}
        {:error, message} -> {:error, message}
      end
    end
  end

  def run(json, user_id) do
    with {:ok, data} <- PortableFile.decode(json) do
      Repo.transaction(fn -> restore(data, user_id) end)
    end
  end

  defp restore(data, user_id) do
    PortableCatalog.restore(data)
    {players, player_counts} = records(data["players"], &player/1)
    {decks, deck_counts} = records(data["decks"], &deck(&1, players))

    {games, game_counts} =
      records(data["games"], &game(&1, players, decks, user_id), "portable_id")

    Enum.each(data["sheet_receipts"], &receipt(&1, games))
    %{players: player_counts, decks: deck_counts, games: game_counts}
  end

  defp records(rows, restore, key \\ "id") do
    Enum.reduce(rows, {%{}, %{created: 0, reused: 0}}, fn row, {mapping, counts} ->
      {record, action} = restore.(row)
      {Map.put(mapping, row[key], record), Map.update!(counts, action, &(&1 + 1))}
    end)
  end

  defp player(row) do
    changeset = Player.changeset(%Player{}, PortableFile.attrs(row, :players))
    validate!(changeset, "Player")
    name = get_field(changeset, :name)

    existing =
      Repo.one(from p in Player, where: fragment("lower(?)", p.name) == ^Games.fold_name(name))

    if existing, do: {existing, :reused}, else: {insert!(changeset, "Player #{name}"), :created}
  end

  defp deck(row, players) do
    player = reference!(players, row["player_id"], "deck owner")
    attrs = row |> PortableFile.attrs(:decks) |> Map.put("player_id", player.id)

    changeset =
      %Deck{}
      |> Deck.changeset(attrs)
      |> cast(attrs, [:skip_count])
      |> validate_required([:skip_count, :included_for_play])
      |> validate_number(:skip_count, greater_than_or_equal_to: 0)

    validate!(changeset, "Deck")
    name = get_field(changeset, :name)
    existing = Games.find_deck(player.id, name)

    if existing do
      if commander_pair(existing) != commander_pair(apply_changes(changeset)),
        do:
          Repo.rollback(
            "#{player.name} already has a deck named #{name} with different commanders. Rename one before importing."
          )

      {existing, :reused}
    else
      {insert!(changeset, "Deck #{name}"), :created}
    end
  end

  defp commander_pair(deck) do
    [deck.commander_name, deck.partner_name]
    |> Enum.reject(&is_nil/1)
    |> Enum.map(&Games.fold_name/1)
    |> Enum.sort()
  end

  defp game(row, players, decks, user_id) do
    seats = Enum.map(row["seats"], &seat(&1, players, decks))
    attrs = row |> PortableFile.attrs(:games) |> Map.put("seats", seats)

    changeset =
      %Game{
        portable_id: row["portable_id"],
        source: row["source"],
        external_id: row["external_id"]
      }
      |> Game.changeset(attrs)
      |> Game.put_created_by(user_id)

    validate!(changeset, "Game #{row["portable_id"]}")
    existing = existing_game(row)
    if existing, do: {existing, :reused}, else: {insert!(changeset, "Game"), :created}
  end

  defp existing_game(row) do
    portable = Repo.get_by(Game, portable_id: row["portable_id"])

    external =
      row["external_id"] &&
        Repo.get_by(Game, source: row["source"], external_id: row["external_id"])

    if portable && external && portable.id != external.id,
      do: Repo.rollback("Game identities refer to different existing games.")

    portable || external
  end

  defp seat(row, players, decks) do
    player = reference!(players, row["player_id"], "seat player")
    deck = optional_reference!(decks, row["deck_id"], "seat deck")

    eliminated_by =
      optional_reference!(players, row["eliminated_by_player_id"], "eliminating player")

    if deck && deck.player_id != player.id,
      do: Repo.rollback("A seat uses another player's deck.")

    row
    |> PortableFile.attrs(:seats)
    |> Map.put("player_id", player.id)
    |> Map.put("deck_id", deck && deck.id)
    |> Map.put("eliminated_by_player_id", eliminated_by && eliminated_by.id)
  end

  defp receipt(row, games) do
    game = reference!(games, row["game_portable_id"], "reconciled game")

    unless is_binary(row["key"]) and row["key"] != "",
      do: Repo.rollback("Invalid sheet receipt key.")

    case Repo.get(SheetReceipt, row["key"]) do
      nil -> Repo.insert!(%SheetReceipt{key: row["key"], game_id: game.id})
      %{game_id: id} when id == game.id -> :ok
      _ -> Repo.rollback("A sheet receipt already belongs to another game.")
    end
  end

  defp optional_reference!(_mapping, nil, _label), do: nil
  defp optional_reference!(mapping, key, label), do: reference!(mapping, key, label)

  defp reference!(mapping, key, label),
    do: mapping[key] || Repo.rollback("Unknown #{label} reference in export.")

  defp insert!(changeset, label) do
    case Repo.insert(changeset) do
      {:ok, value} -> value
      {:error, changeset} -> invalid!(changeset, label)
    end
  end

  defp validate!(%{valid?: true}, _label), do: :ok
  defp validate!(changeset, label), do: invalid!(changeset, label)

  defp invalid!(changeset, label) do
    errors = traverse_errors(changeset, fn {message, _opts} -> message end)
    Repo.rollback("#{label}: #{inspect(errors)}")
  end
end
