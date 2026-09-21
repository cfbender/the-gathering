defmodule TheGathering.Imports.SheetResolution do
  @moduledoc false
  alias TheGathering.Games
  alias TheGathering.Imports.SheetMatch

  def resolve(row, params, players, decks, candidates, imported_id) do
    choice = get_in(params, ["actions", row.key])
    unresolved = Enum.map(row.seats, &resolve_seat(&1, params, players, decks, nil))
    {matched, reason} = SheetMatch.find(row, unresolved, candidates, decks)
    target = chosen_target(choice, candidates, matched)
    action = choice || if(target, do: target.id, else: "skip")

    seats = Enum.map(row.seats, &resolve_seat(&1, params, players, decks, target))
    kills = Enum.map(row.kill_counts, &{player_id(&1.player, params, players), &1.kills})
    seats = Enum.map(seats, &Map.put(&1, :kills, kill_count(kills, &1.player_id)))
    errors = row.errors ++ validate(seats, kills, action, target)
    notes = notes(row)
    changes = changes(target, seats, notes, decks)
    status = status(imported_id, errors, target, changes)

    row
    |> Map.merge(%{
      seats: seats,
      action: reviewed_action(action, choice, status),
      status: status,
      match_reason: if(is_integer(choice), do: "Manually selected game", else: reason),
      changes: changes,
      imported_id: imported_id,
      candidates: candidates,
      errors: errors,
      notes: notes,
      target: target
    })
  end

  defp chosen_target(choice, candidates, _matched) when is_integer(choice),
    do: Enum.find(candidates, &(&1.id == choice))

  defp chosen_target("create", _candidates, _matched), do: nil
  defp chosen_target(_choice, _candidates, matched), do: matched

  defp reviewed_action(_action, _choice, status) when status in ["unchanged", "reconciled"],
    do: "skip"

  defp reviewed_action(_action, nil, "review"), do: "skip"
  defp reviewed_action(action, _choice, _status), do: action

  defp status(imported_id, _errors, _target, _changes) when not is_nil(imported_id),
    do: "reconciled"

  defp status(_imported_id, [_ | _], _target, _changes), do: "review"
  defp status(_imported_id, _errors, nil, _changes), do: "review"
  defp status(_imported_id, _errors, _target, []), do: "unchanged"
  defp status(_imported_id, _errors, _target, _changes), do: "changed"

  defp changes(nil, _seats, _notes, _decks), do: []

  defp changes(target, seats, notes, decks) do
    seat_changes =
      Enum.flat_map(seats, fn seat ->
        existing = Enum.find(target.seats, &(&1.player_id == seat.player_id))
        seat_changes(existing, seat, decks)
      end)

    diff(seat_changes, "notes", nil, target.notes, if(notes == "", do: target.notes, else: notes))
  end

  defp seat_changes(nil, _seat, _decks), do: []

  defp seat_changes(existing, seat, decks) do
    changes =
      []
      |> diff("result", existing.player, existing.result, seat.result)
      |> diff("kills", existing.player, existing.kills, seat.kills)

    if existing.deck_id == seat.deck_id do
      changes
    else
      deck = Enum.find(decks, &(&1.id == seat.deck_id))

      diff(
        changes,
        "deck",
        existing.player,
        existing.deck,
        if(deck, do: deck.name, else: seat.deck)
      )
    end
  end

  defp diff(changes, _field, _player, same, same), do: changes

  defp diff(changes, field, player, before, after_value),
    do: changes ++ [%{field: field, player: player, before: before, after: after_value}]

  defp resolve_seat(seat, params, players, decks, target) do
    player_id = player_id(seat.player, params, players)
    key = Jason.encode!([seat.player, seat.deck])
    choice = reuse_deck(get_in(params, ["decks", key]), player_id, seat.deck)
    existing = target && Enum.find(target.seats, &(&1.player_id == player_id))

    exact =
      Enum.find(
        decks,
        &(&1.player_id == player_id and Games.fold_name(&1.name) == Games.fold_name(seat.deck))
      )

    deck_id = choice || if(existing, do: existing.deck_id, else: exact && exact.id)

    Map.merge(seat, %{
      player_id: player_id,
      deck_key: key,
      deck_id: deck_id,
      deck_valid: valid_deck?(deck_id, player_id, decks, existing)
    })
  end

  defp reuse_deck("new", player_id, name) when is_integer(player_id) do
    case Games.find_deck(player_id, name, name) do
      nil -> "new"
      deck -> deck.id
    end
  end

  defp reuse_deck(choice, _player_id, _name), do: choice

  defp valid_deck?("new", _player_id, _decks, _existing), do: true
  defp valid_deck?(nil, _player_id, _decks, existing), do: not is_nil(existing)

  defp valid_deck?(id, player_id, decks, _existing),
    do: Enum.any?(decks, &(&1.id == id and &1.player_id == player_id))

  defp player_id(name, params, players) do
    case get_in(params, ["players", name]) do
      "new" ->
        "new:" <> Games.fold_name(name)

      id when is_integer(id) ->
        if Enum.any?(players, &(&1.id == id)), do: id

      _ ->
        case Enum.find(players, &(Games.fold_name(&1.name) == Games.fold_name(name))) do
          nil -> nil
          player -> player.id
        end
    end
  end

  defp kill_count(kills, player_id) do
    case Enum.find(kills, fn {id, _count} -> id != nil and id == player_id end) do
      nil -> 0
      {_id, count} -> count
    end
  end

  defp validate(seats, kills, action, target) do
    ids = Enum.map(seats, & &1.player_id)
    recorded_ids = Enum.map(kills, &elem(&1, 0)) |> Enum.reject(&is_nil/1)

    []
    |> error(nil in ids, "Map every player to an existing player or explicitly create one.")
    |> error(
      Enum.uniq(ids) != ids,
      "Player aliases resolve to the same player twice. Repair this row."
    )
    |> error(Enum.any?(seats, &(not &1.deck_valid)), "Map missing decks before creating games.")
    |> error(
      Enum.any?(kills, fn {id, count} -> count > 0 and (is_nil(id) or id not in ids) end),
      "A positive kill count belongs to someone not seated. Fix the column or player mapping."
    )
    |> error(
      Enum.uniq(recorded_ids) != recorded_ids,
      "Multiple kill columns map to the same player."
    )
    |> error(
      action not in ["skip", "create"] and is_nil(target),
      "Choose a game from the nearby dates."
    )
    |> error(
      not is_nil(target) and Enum.sort(ids) != Enum.sort(Enum.map(target.seats, & &1.player_id)),
      "Player lists differ. Correct this row or edit the existing game before reconciling."
    )
  end

  defp error(errors, true, message), do: errors ++ [message]
  defp error(errors, false, _message), do: errors

  defp notes(row) do
    [if(row.win_con not in ["", "N/A"], do: "Win con: " <> row.win_con), row.notes]
    |> Enum.reject(&(&1 in [nil, ""]))
    |> Enum.join("\n")
  end
end
