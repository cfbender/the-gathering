defmodule TheGathering.Stats.Query do
  @moduledoc "Scoped and date-filtered queries shared by statistics views."

  import Ecto.Query

  alias TheGathering.Catalog.CardData
  alias TheGathering.Games.{Deck, Game, GamePlayer, Player}
  alias TheGathering.Repo

  def games(params, filters \\ []) do
    from(game in Game, as: :game)
    |> date_range(params)
    |> maybe_player(filters[:player_id])
    |> maybe_deck(filters[:deck_id])
    |> order_by([game], desc: game.played_at, desc: game.id)
    |> preload(seats: [:player, :deck])
    |> Repo.all()
  end

  def commander_seats(params) do
    GamePlayer
    |> join(:inner, [seat], game in assoc(seat, :game), as: :game)
    |> join(:inner, [seat], _deck in assoc(seat, :deck), as: :deck)
    |> date_range(params)
    |> order_by([seat, game: game], desc: game.played_at, desc: game.id, asc: seat.seat)
    |> preload([:player, :deck, :game])
    |> Repo.all()
  end

  def commander_seats(params, ids, names) do
    normalized_names = Enum.map(names, &String.downcase/1)

    GamePlayer
    |> join(:inner, [seat], game in assoc(seat, :game), as: :game)
    |> join(:inner, [seat], deck in assoc(seat, :deck), as: :deck)
    |> date_range(params)
    |> where(
      [deck: deck],
      deck.commander_card_id in ^ids or deck.partner_card_id in ^ids or
        fragment("lower(?)", deck.commander_name) in ^normalized_names or
        fragment("lower(?)", deck.partner_name) in ^normalized_names
    )
    |> order_by([seat, game: game], desc: game.played_at, desc: game.id, asc: seat.seat)
    |> preload([:player, :deck, :game])
    |> Repo.all()
  end

  @doc "Stored commander references that directly match an ID or normalized card name."
  def commander_references(id) do
    normalized = CardData.normalize_name(id)
    downcased = String.downcase(id)

    references =
      Deck
      |> where(
        [deck],
        deck.commander_card_id == ^id or deck.partner_card_id == ^id or
          fragment("lower(?)", deck.commander_name) == ^downcased or
          fragment("lower(?)", deck.partner_name) == ^downcased
      )
      |> select(
        [deck],
        {deck.commander_card_id, deck.commander_name, deck.partner_card_id, deck.partner_name}
      )
      |> Repo.all()
      |> matching_references(id, normalized)

    if references == [] do
      all_commander_references()
      |> Enum.filter(fn {stored_id, name} ->
        reference_matches?(stored_id, name, id, normalized)
      end)
    else
      references
    end
  end

  @doc "All stored IDs and exact name spellings that canonicalize to one commander."
  def commander_aliases(ids, names) do
    normalized_names = MapSet.new(names, &CardData.normalize_name/1)

    references =
      all_commander_references()
      |> Enum.filter(fn {stored_id, name} ->
        stored_id in ids or
          (is_binary(name) and MapSet.member?(normalized_names, CardData.normalize_name(name)))
      end)

    {
      references |> Enum.map(&elem(&1, 0)) |> Enum.reject(&is_nil/1) |> Enum.uniq(),
      references |> Enum.map(&elem(&1, 1)) |> Enum.reject(&is_nil/1) |> Enum.uniq()
    }
  end

  def recent_games(game_ids, limit) do
    Game
    |> where([game], game.id in ^game_ids)
    |> order_by([game], desc: game.played_at, desc: game.id)
    |> limit(^limit)
    |> preload(seats: [:player, :deck])
    |> Repo.all()
  end

  @doc """
  Per opponent player across `game_ids`: their own record in those games, plus
  `beaten`, the number of those games a tracked seat won against them.
  """
  def opponent_counts(game_ids, tracked_seat_ids) do
    GamePlayer
    |> join(:inner, [seat], player in Player, on: player.id == seat.player_id)
    |> join(:left, [seat], winner in GamePlayer,
      on:
        winner.game_id == seat.game_id and winner.result == "win" and
          winner.id in ^tracked_seat_ids,
      as: :winner
    )
    |> where(
      [seat],
      seat.game_id in ^game_ids and seat.id not in ^tracked_seat_ids
    )
    |> group_by([_seat, player], [player.id, player.name])
    |> select([seat, player, winner: winner], %{
      id: player.id,
      name: player.name,
      games: count(seat.id),
      wins: fragment("SUM(CASE WHEN ? = 'win' THEN 1 ELSE 0 END)", seat.result),
      losses: fragment("SUM(CASE WHEN ? = 'loss' THEN 1 ELSE 0 END)", seat.result),
      draws: fragment("SUM(CASE WHEN ? = 'draw' THEN 1 ELSE 0 END)", seat.result),
      beaten: count(winner.id)
    })
    |> Repo.all()
  end

  @doc """
  Applies optional inclusive `date_from` / `date_to` ISO dates from `params` to a query
  whose game binding is named `:game`. Unparseable values are ignored.
  """
  def date_range(query, params) do
    query
    |> maybe_date_from(params_value(params, :date_from))
    |> maybe_date_to(params_value(params, :date_to))
  end

  defp params_value(params, key), do: Map.get(params, key) || Map.get(params, Atom.to_string(key))

  defp maybe_date_from(query, nil), do: query

  defp maybe_date_from(query, value) do
    case Date.from_iso8601(value) do
      {:ok, date} -> where(query, [game: game], game.played_at >= ^start_of_day(date))
      _error -> query
    end
  end

  defp maybe_date_to(query, nil), do: query

  defp maybe_date_to(query, value) do
    case Date.from_iso8601(value) do
      {:ok, date} -> where(query, [game: game], game.played_at < ^start_of_day(Date.add(date, 1)))
      _error -> query
    end
  end

  defp start_of_day(date), do: DateTime.new!(date, ~T[00:00:00], "Etc/UTC")

  defp all_commander_references do
    Deck
    |> select(
      [deck],
      {deck.commander_card_id, deck.commander_name, deck.partner_card_id, deck.partner_name}
    )
    |> Repo.all()
    |> Enum.flat_map(fn {commander_id, commander_name, partner_id, partner_name} ->
      [{commander_id, commander_name}, {partner_id, partner_name}]
    end)
    |> Enum.reject(fn {id, name} -> is_nil(id) and (is_nil(name) or name == "") end)
    |> Enum.uniq()
  end

  defp matching_references(rows, id, normalized) do
    rows
    |> Enum.flat_map(fn {commander_id, commander_name, partner_id, partner_name} ->
      [{commander_id, commander_name}, {partner_id, partner_name}]
    end)
    |> Enum.filter(fn {stored_id, name} ->
      reference_matches?(stored_id, name, id, normalized)
    end)
    |> Enum.uniq()
  end

  defp reference_matches?(stored_id, name, id, normalized) do
    stored_id == id or (is_binary(name) and CardData.normalize_name(name) == normalized)
  end

  defp maybe_player(query, nil), do: query

  defp maybe_player(query, id) do
    where(
      query,
      [game],
      game.id in subquery(
        from seat in GamePlayer, where: seat.player_id == ^id, select: seat.game_id
      )
    )
  end

  defp maybe_deck(query, nil), do: query

  defp maybe_deck(query, id) do
    where(
      query,
      [game],
      game.id in subquery(
        from seat in GamePlayer, where: seat.deck_id == ^id, select: seat.game_id
      )
    )
  end
end
