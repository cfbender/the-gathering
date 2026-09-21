defmodule TheGathering.Imports.CSVTransfer do
  @moduledoc false
  import Ecto.Query

  alias TheGathering.Games
  alias TheGathering.Games.{Deck, Game, GamePlayer, LinkCatalogCards, Player}
  alias TheGathering.Imports.{Commit, CSVChanges, Preview}
  alias TheGathering.Repo

  def preview(csv) do
    base = Preview.run(:csv, csv)

    if base.valid do
      result =
        Repo.transaction(fn ->
          revision = revision(csv)
          review = execute(base.games, nil)
          Repo.rollback({:preview, Map.merge(base, %{revision: revision, review: review})})
        end)

      case result do
        {:error, {:preview, preview}} -> preview
        {:error, reason} -> invalid(base, reason)
      end
    else
      base
    end
  end

  def run(csv, user_id, reviewed_revision) do
    base = Preview.run(:csv, csv)

    if base.valid do
      result =
        Repo.transaction(fn ->
          check_revision!(base.games, csv, reviewed_revision)
          execute(base.games, user_id)
        end)

      finish(result, base)
    else
      {:error, {:validation, base}}
    end
  end

  defp finish({:ok, review}, _base) do
    ids = Enum.map(review, & &1.target_id) |> Enum.reject(&is_nil/1)

    review
    |> Enum.reject(&(&1.action == "skip"))
    |> Enum.each(&LinkCatalogCards.link_game(&1.target_id))

    {:ok,
     %{
       created: Enum.count(review, &(&1.action == "create")),
       updated: Enum.count(review, &(&1.action == "update")),
       skipped: Enum.count(review, &(&1.action == "skip")),
       game_ids: ids
     }}
  end

  defp finish({:error, reason}, base), do: {:error, {:validation, invalid(base, reason)}}

  defp check_revision!(games, csv, reviewed_revision) do
    if Enum.any?(games, &(&1.action == "update")) and reviewed_revision != revision(csv),
      do: Repo.rollback("Preview is stale or missing. Preview again before updating games.")
  end

  defp execute(games, user_id) do
    targets = Enum.map(games, &{&1, target(&1)})
    ids = for {game, target} <- targets, game.action != "skip" and target != nil, do: target.id

    if length(Enum.uniq(ids)) != length(ids),
      do: Repo.rollback("Multiple CSV games target the same existing game.")

    Enum.map(targets, fn {game, target} -> transfer(game, target, user_id) end)
  end

  defp target(%{action: "skip"}), do: nil

  defp target(game) do
    portable = game.target_portable_id && Repo.get_by(Game, portable_id: game.target_portable_id)
    source = game.target_source || "csv"

    external =
      Repo.get_by(Game, source: source, external_id: game.target_external_id || game.external_id)

    validate_identity!(game, portable, external)
    found = portable || external

    if game.action == "update" and is_nil(found),
      do:
        Repo.rollback(
          "Game #{game.game_id}: update target was not found; no game will be created."
        )

    found && Games.get_game!(found.id)
  end

  defp validate_identity!(game, portable, external) do
    if portable && external && portable.id != external.id,
      do: Repo.rollback("Game identities refer to different existing games.")

    if game.action == "update" && game.target_external_id && is_nil(external),
      do: Repo.rollback("Game #{game.game_id}: source/external_id was not found.")

    if game.target_portable_id && is_nil(portable),
      do: Repo.rollback("Game #{game.game_id}: portable ID was not found.")
  end

  defp transfer(%{action: "skip"} = game, _target, _user_id), do: review(game, nil, "skip", [])

  defp transfer(%{action: "create"} = game, %Game{} = target, _user_id),
    do: review(game, target, "skip", [])

  defp transfer(game, target, user_id) do
    seats = Enum.map(game.seats, &seat/1)

    attrs =
      game
      |> Map.take([:played_at, :duration_minutes, :turns, :win_condition, :notes])
      |> Map.reject(fn {_key, value} -> is_nil(value) end)

    if target do
      attrs = Map.put(attrs, :seats, update_seats(seats, target))

      projected =
        target |> Game.changeset(attrs) |> Ecto.Changeset.apply_action(:update) |> unwrap!()

      projected = Map.update!(projected, :seats, &Repo.preload(&1, [:player, :deck], force: true))
      changes = CSVChanges.diff(target, projected)

      if changes == [] do
        review(game, target, "skip", [])
      else
        prepare_seats(target, seats)
        saved = target.id |> Games.get_game!() |> Games.update_game(attrs) |> unwrap!()
        review(game, saved, "update", changes)
      end
    else
      attrs =
        Map.merge(attrs, %{
          seats: seats,
          source: game.target_source || "csv",
          external_id: game.target_external_id || game.external_id
        })

      saved = Games.create_game(attrs, user_id) |> unwrap!()
      review(game, saved, "create", [])
    end
  end

  defp seat(seat) do
    # Reuse by commander pair when possible, but never rewrite a shared deck's commander.
    player =
      Repo.one(
        from p in Player, where: fragment("lower(?)", p.name) == ^Games.fold_name(seat.player)
      )

    deck = player && Games.find_deck(player, seat.deck)

    if deck &&
         pair(deck.commander_name, deck.partner_name) != pair(seat.commander, seat.partner_name),
       do:
         Repo.rollback(
           "#{seat.player}: deck #{seat.deck} has different commanders. Use a different deck name."
         )

    Commit.commit_seat(seat)
  end

  defp pair(commander, partner),
    do:
      [commander, partner]
      |> Enum.reject(&is_nil/1)
      |> Enum.map(&Games.fold_name/1)
      |> Enum.sort()

  defp update_seats(seats, target) do
    ids = Enum.map(seats, & &1.player_id)

    Enum.map(seats, fn seat ->
      existing = Enum.find(target.seats, &(&1.player_id == seat.player_id))
      merge_seat(seat, existing, ids)
    end)
  end

  defp merge_seat(seat, nil, _ids), do: seat

  defp merge_seat(seat, existing, ids) do
    seat = seat |> Map.reject(fn {_key, value} -> is_nil(value) end) |> Map.put(:id, existing.id)

    if existing.eliminated_by_player_id && existing.eliminated_by_player_id not in ids,
      do: Map.put(seat, :eliminated_by_player_id, nil),
      else: seat
  end

  defp prepare_seats(target, seats) do
    ids = Enum.map(seats, & &1.player_id)
    # Avoid unique-index collisions while swapping seats. The enclosing transaction
    # rolls this back along with all other writes if any validation fails.
    Repo.delete_all(
      from s in GamePlayer, where: s.game_id == ^target.id and s.player_id not in ^ids
    )

    reordered? =
      Enum.any?(seats, fn seat ->
        Enum.any?(target.seats, &(&1.player_id == seat.player_id and &1.seat != seat.seat))
      end)

    if reordered?,
      do: Repo.update_all(from(s in GamePlayer, where: s.game_id == ^target.id), inc: [seat: 10])
  end

  defp review(game, target, action, changes),
    do: %{game_id: game.game_id, target_id: target && target.id, action: action, changes: changes}

  defp revision(csv) do
    snapshot =
      Enum.map([Game, GamePlayer, Player, Deck], fn schema ->
        Repo.all(from row in schema, order_by: row.id)
      end)

    :crypto.hash(:sha256, :erlang.term_to_binary({csv, snapshot})) |> Base.encode16(case: :lower)
  end

  defp invalid(base, reason),
    do: %{
      base
      | valid: false,
        errors: base.errors ++ [%{line: 1, field: "import", message: message(reason)}]
    }

  defp message(%Ecto.Changeset{} = changeset),
    do: inspect(Ecto.Changeset.traverse_errors(changeset, fn {message, _} -> message end))

  defp message(reason), do: to_string(reason)
  defp unwrap!({:ok, record}), do: record
  defp unwrap!({:error, reason}), do: Repo.rollback(reason)
end
