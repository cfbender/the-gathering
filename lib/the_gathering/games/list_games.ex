defmodule TheGathering.Games.ListGames do
  @moduledoc """
  Filtered, paginated game history behind `GET /api/games`, newest first.

  Deck filters (`commander`, `colors`, `color`) match the `player_id` seat when a
  player is chosen, so "Alice with Golgari" means Alice piloted Golgari; otherwise they
  match any seat. Winner filters (`winner_id`, `winner_colors`, `winner_color`,
  `winner_seat`) all describe one winning seat. Date, weekday, and hour filters read
  `played_at` in the `tz` IANA zone, defaulting to UTC.
  """

  import Ecto.Query

  alias TheGathering.Games.{ColorIdentity, Game, GamePlayer}
  alias TheGathering.{LocalTime, Repo}

  @results ~w(win loss draw)
  @colors ~w(W U B R G)

  def call(opts) do
    page = positive_integer(value(opts, :page)) || 1
    per_page = opts |> value(:per_page) |> positive_integer() |> Kernel.||(20) |> min(100)
    zone = LocalTime.zone(value(opts, :tz))

    query =
      opts
      |> seat_conditions()
      |> Enum.reduce(Game, &where_seat(&2, &1))
      |> maybe_win_condition(value(opts, :win_condition))
      |> maybe_player_count(positive_integer(value(opts, :player_count)))
      |> maybe_minimum(:turns, positive_integer(value(opts, :min_turns)))
      |> maybe_maximum(:turns, positive_integer(value(opts, :max_turns)))
      |> maybe_minimum(:duration_minutes, positive_integer(value(opts, :min_duration)))
      |> maybe_maximum(:duration_minutes, positive_integer(value(opts, :max_duration)))
      |> maybe_date_from(date(value(opts, :date_from)), zone)
      |> maybe_date_to(date(value(opts, :date_to)), zone)
      |> maybe_local_clock(
        integer_in(value(opts, :weekday), 0..6),
        integer_in(value(opts, :hour), 0..23),
        zone
      )
      |> order_by([game], desc: game.played_at, desc: game.id)

    total = Repo.aggregate(query, :count, :id)

    games =
      query
      |> limit(^per_page)
      |> offset(^((page - 1) * per_page))
      |> preload(seats: [:player, :deck])
      |> Repo.all()

    {games,
     %{page: page, per_page: per_page, total: total, total_pages: max(ceil(total / per_page), 1)}}
  end

  # Each returned condition must hold for one seat. Deck filters describe the chosen
  # player's seat when there is one, and winner filters all describe one winning seat.
  defp seat_conditions(opts) do
    deck_filters =
      compact([
        commander(value(opts, :commander)),
        identity(value(opts, :colors)),
        includes_color(value(opts, :color))
      ])

    player_seat =
      case player(positive_integer(value(opts, :player_id)), result(value(opts, :player_result))) do
        nil -> deck_filters
        condition -> [all([condition | deck_filters])]
      end

    winner_seat =
      compact([
        player(positive_integer(value(opts, :winner_id)), nil),
        identity(value(opts, :winner_colors)),
        includes_color(value(opts, :winner_color)),
        seat_number(positive_integer(value(opts, :winner_seat)))
      ])

    compact([
      winning(all(winner_seat)),
      player(positive_integer(value(opts, :opponent_id)), nil),
      deck(positive_integer(value(opts, :deck_id)))
    ]) ++ player_seat
  end

  defp compact(conditions), do: Enum.reject(conditions, &is_nil/1)

  defp all([]), do: nil
  defp all([first | rest]), do: Enum.reduce(rest, first, &dynamic(^&2 and ^&1))

  defp where_seat(query, condition) do
    game_ids =
      from seat in GamePlayer,
        left_join: deck in assoc(seat, :deck),
        as: :deck,
        where: ^condition,
        select: seat.game_id

    where(query, [game], game.id in subquery(game_ids))
  end

  defp player(nil, _result), do: nil
  defp player(id, nil), do: dynamic([seat], seat.player_id == ^id)
  defp player(id, result), do: dynamic([seat], seat.player_id == ^id and seat.result == ^result)

  defp winning(nil), do: nil
  defp winning(condition), do: dynamic([seat], seat.result == "win" and ^condition)

  defp deck(nil), do: nil
  defp deck(id), do: dynamic([seat], seat.deck_id == ^id)

  defp seat_number(nil), do: nil
  defp seat_number(number), do: dynamic([seat], seat.seat == ^number)

  defp commander(name) when is_binary(name) do
    case String.trim(name) do
      "" ->
        nil

      trimmed ->
        needle = String.downcase(trimmed)

        dynamic(
          [deck: deck],
          fragment("instr(lower(?), ?) > 0", deck.commander_name, ^needle) or
            fragment("instr(lower(coalesce(?, '')), ?) > 0", deck.partner_name, ^needle)
        )
    end
  end

  defp commander(_name), do: nil

  # Exact color identity: canonical WUBRG letters, or "C" for colorless. Stored identities
  # are validated to unique WUBRG letters but not ordered, so compare length and membership.
  defp identity(value) when is_binary(value) do
    case value |> String.trim() |> String.upcase() do
      "C" ->
        dynamic([deck: deck], not is_nil(deck.id) and coalesce(deck.color_identity, "") == "")

      letters ->
        if Regex.match?(~r/^[WUBRG]+$/, letters) do
          canonical = ColorIdentity.canonical(letters)

          canonical
          |> String.graphemes()
          |> Enum.reduce(
            dynamic(
              [deck: deck],
              fragment("length(?)", deck.color_identity) == ^String.length(canonical)
            ),
            &dynamic([deck: deck], ^&2 and fragment("instr(?, ?) > 0", deck.color_identity, ^&1))
          )
        end
    end
  end

  defp identity(_value), do: nil

  defp includes_color(value) when is_binary(value) do
    color = value |> String.trim() |> String.upcase()

    if color in @colors,
      do: dynamic([deck: deck], fragment("instr(?, ?) > 0", deck.color_identity, ^color))
  end

  defp includes_color(_value), do: nil

  defp result(value) when value in @results, do: value
  defp result(_value), do: nil

  defp maybe_win_condition(query, value) when is_binary(value) and value != "",
    do: where(query, [game], game.win_condition == ^value)

  defp maybe_win_condition(query, _value), do: query

  defp maybe_player_count(query, nil), do: query

  defp maybe_player_count(query, count) do
    game_ids =
      from seat in GamePlayer,
        group_by: seat.game_id,
        having: count(seat.id) == ^count,
        select: seat.game_id

    where(query, [game], game.id in subquery(game_ids))
  end

  defp maybe_minimum(query, _field, nil), do: query
  defp maybe_minimum(query, field, min), do: where(query, [game], field(game, ^field) >= ^min)

  defp maybe_maximum(query, _field, nil), do: query
  defp maybe_maximum(query, field, max), do: where(query, [game], field(game, ^field) <= ^max)

  defp maybe_date_from(query, nil, _zone), do: query

  defp maybe_date_from(query, date, zone),
    do: where(query, [game], game.played_at >= ^LocalTime.start_of_day(date, zone))

  defp maybe_date_to(query, nil, _zone), do: query

  defp maybe_date_to(query, date, zone),
    do: where(query, [game], game.played_at < ^LocalTime.start_of_day(Date.add(date, 1), zone))

  # SQLite has no time zone data, so match weekday and hour in Elixir over the games the
  # SQL filters already narrowed down, then constrain the query to those IDs.
  defp maybe_local_clock(query, nil, nil, _zone), do: query

  defp maybe_local_clock(query, weekday, hour, zone) do
    ids =
      query
      |> select([game], {game.id, game.played_at})
      |> Repo.all()
      |> Enum.filter(fn {_id, played_at} ->
        local = LocalTime.to_local(played_at, zone)
        weekday in [nil, LocalTime.weekday(local)] and hour in [nil, local.hour]
      end)
      |> Enum.map(&elem(&1, 0))

    where(query, [game], game.id in ^ids)
  end

  defp date(%Date{} = date), do: date

  defp date(value) when is_binary(value) do
    case Date.from_iso8601(value) do
      {:ok, date} -> date
      _error -> nil
    end
  end

  defp date(_value), do: nil

  defp positive_integer(value) do
    case integer(value) do
      integer when is_integer(integer) and integer > 0 -> integer
      _other -> nil
    end
  end

  defp integer_in(value, range) do
    case integer(value) do
      integer when is_integer(integer) -> if integer in range, do: integer
      nil -> nil
    end
  end

  defp integer(value) when is_integer(value), do: value

  defp integer(value) when is_binary(value) do
    case Integer.parse(value) do
      {integer, ""} -> integer
      _other -> nil
    end
  end

  defp integer(_value), do: nil

  defp value(map, key), do: Map.get(map, key, Map.get(map, Atom.to_string(key)))
end
