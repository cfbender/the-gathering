defmodule TheGathering.Imports.CSV do
  @moduledoc false

  alias TheGathering.Games

  NimbleCSV.define(Parser, separator: ",", escape: "\"")

  @native_required ~w(gameid date player deck commander seat result)
  @mythic_required ~w(date player1 player2 player1commander player2commander winner)

  def parse(csv) do
    rows = Parser.parse_string(csv, skip_headers: false)

    case rows do
      [] -> {:error, [error(1, "csv", "must include a header row")]}
      [headers | data] -> parse_rows(headers, data)
    end
  rescue
    exception in NimbleCSV.ParseError ->
      {:error, [error(1, "csv", Exception.message(exception))]}
  end

  defp parse_rows(headers, rows) do
    normalized = Enum.map(headers, &normalize_header/1)

    cond do
      Enum.all?(@native_required, &(&1 in normalized)) -> parse_native(normalized, rows)
      Enum.all?(@mythic_required, &(&1 in normalized)) -> parse_mythic(normalized, rows)
      true -> {:error, [error(1, "headers", header_error(normalized))]}
    end
  end

  defp parse_native(headers, rows) do
    {parsed, errors} =
      rows
      |> Enum.with_index(2)
      |> Enum.reject(fn {row, _line} -> Enum.all?(row, &(String.trim(&1) == "")) end)
      |> Enum.map(fn {row, line} -> native_row(headers, row, line) end)
      |> collect_rows()

    games = build_games(parsed)
    game_errors = empty_error(games, errors) ++ Enum.flat_map(games, &validate_game/1)
    {:ok, games, errors ++ game_errors}
  end

  defp native_row(headers, row, line) do
    values = row_map(headers, row)

    parsed = %{
      game_id: value(values, "gameid"),
      date: parse_date(value(values, "date")),
      player: value(values, "player"),
      deck: value(values, "deck"),
      commander: value(values, "commander"),
      seat: parse_integer(value(values, "seat")),
      result: values |> value("result") |> String.downcase(),
      mvp_card: blank_to_nil(value(values, "mvpcard")),
      duration_minutes: parse_optional_integer(value(values, "durationminutes")),
      turns: parse_optional_integer(value(values, "turns")),
      notes: blank_to_nil(value(values, "notes")),
      line: line
    }

    {parsed, validate_row(parsed)}
  end

  defp parse_mythic(headers, rows) do
    {parsed, errors} =
      rows
      |> Enum.with_index(2)
      |> Enum.reject(fn {row, _line} -> Enum.all?(row, &(String.trim(&1) == "")) end)
      |> Enum.flat_map(fn {row, line} -> mythic_rows(headers, row, line) end)
      |> collect_rows()

    games = build_games(parsed)
    game_errors = empty_error(games, errors) ++ Enum.flat_map(games, &validate_game/1)
    {:ok, games, errors ++ game_errors}
  end

  defp mythic_rows(headers, row, line) do
    values = row_map(headers, row)
    players = for index <- 1..4, value(values, "player#{index}") != "", do: index
    winner = value(values, "winner")

    game_id =
      "mythic-#{line}-#{value(values, "date")}-#{Enum.map_join(players, "-", &value(values, "player#{&1}"))}"

    Enum.map(players, fn index ->
      player = value(values, "player#{index}")
      commander = value(values, "player#{index}commander")

      parsed = %{
        game_id: game_id,
        date: parse_date(value(values, "date")),
        player: player,
        deck: commander,
        commander: commander,
        seat: index,
        result: mythic_result(winner, player),
        mvp_card: nil,
        duration_minutes: parse_optional_integer(value(values, "gametimeminutes")),
        turns: parse_optional_integer(value(values, "totalturns")),
        notes: blank_to_nil(value(values, "notes")),
        line: line
      }

      {parsed, validate_row(parsed)}
    end)
  end

  defp build_games(rows) do
    rows
    |> Enum.group_by(& &1.game_id)
    |> Enum.map(fn {_game_id, seats} -> build_game(seats) end)
    |> Enum.sort_by(&{&1.played_at, &1.external_id})
  end

  defp build_game(seats) do
    first = hd(seats)

    normalized =
      seats
      |> Enum.sort_by(& &1.seat)
      |> Enum.map_join("\n", fn seat ->
        [
          first.game_id,
          datetime_string(first.date),
          seat.player,
          seat.deck,
          seat.commander,
          seat.seat,
          seat.result,
          seat.mvp_card,
          first.duration_minutes,
          first.turns,
          first.notes
        ]
        |> Enum.map_join("|", &to_string(&1 || ""))
      end)

    %{
      external_id: Base.encode16(:crypto.hash(:sha256, normalized), case: :lower),
      game_id: first.game_id,
      played_at: first.date,
      duration_minutes: first.duration_minutes,
      turns: first.turns,
      notes: first.notes,
      lines: seats |> Enum.map(& &1.line) |> Enum.uniq() |> Enum.sort(),
      seats: Enum.sort_by(seats, & &1.seat)
    }
  end

  defp validate_row(row) do
    []
    |> required(row.line, "game_id", row.game_id)
    |> required(row.line, "date", row.date)
    |> required(row.line, "player", row.player)
    |> required(row.line, "deck", row.deck)
    |> required(row.line, "commander", row.commander)
    |> max_length(row.line, "player", row.player, 100)
    |> max_length(row.line, "deck", row.deck, 100)
    |> valid_positive(row.line, "seat", row.seat)
    |> valid_result(row.line, row.result)
    |> valid_optional_positive(row.line, "duration_minutes", row.duration_minutes)
    |> valid_optional_positive(row.line, "turns", row.turns)
  end

  defp validate_game(game) do
    seats = game.seats
    lines = game.lines

    []
    |> add_game_error(
      length(seats) not in 2..6,
      lines,
      "game_id",
      "must contain between 2 and 6 players"
    )
    |> add_game_error(
      duplicate?(seats, &Games.fold_name(&1.player)),
      lines,
      "player",
      "cannot contain the same player twice"
    )
    |> add_game_error(
      Enum.map(seats, & &1.seat) |> Enum.sort() != Enum.to_list(1..length(seats)),
      lines,
      "seat",
      "must use consecutive seat numbers starting at 1"
    )
    |> add_game_error(
      not valid_results?(seats),
      lines,
      "result",
      "must have exactly one winner and all other players lose, or all players draw"
    )
    |> add_game_error(
      not consistent?(seats, :date),
      lines,
      "date",
      "must match for every row in the game"
    )
    |> add_game_error(
      not consistent?(seats, :duration_minutes),
      lines,
      "duration_minutes",
      "must match for every row in the game"
    )
    |> add_game_error(
      not consistent?(seats, :turns),
      lines,
      "turns",
      "must match for every row in the game"
    )
    |> add_game_error(
      not consistent?(seats, :notes),
      lines,
      "notes",
      "must match for every row in the game"
    )
  end

  defp collect_rows(items) do
    Enum.reduce(items, {[], []}, fn {row, errors}, {rows, all_errors} ->
      if errors == [], do: {[row | rows], all_errors}, else: {rows, all_errors ++ errors}
    end)
    |> then(fn {rows, errors} -> {Enum.reverse(rows), errors} end)
  end

  defp row_map(headers, row), do: headers |> Enum.zip(row) |> Map.new()
  defp value(values, key), do: values |> Map.get(key, "") |> String.trim()

  defp normalize_header(header),
    do: header |> String.trim() |> String.downcase() |> String.replace(~r/[^a-z0-9]/, "")

  defp blank_to_nil(""), do: nil
  defp blank_to_nil(value), do: value

  defp parse_date(""), do: :invalid

  defp parse_date(value) do
    with {:error, _reason} <- DateTime.from_iso8601(value),
         {:error, _reason} <- Date.from_iso8601(value),
         [month, day, year] <- String.split(value, "/"),
         {month, ""} <- Integer.parse(month),
         {day, ""} <- Integer.parse(day),
         {year, ""} <- Integer.parse(year),
         {:ok, date} <- Date.new(year, month, day) do
      DateTime.new!(date, ~T[12:00:00], "Etc/UTC")
    else
      {:ok, datetime, _offset} -> datetime
      {:ok, date} -> DateTime.new!(date, ~T[12:00:00], "Etc/UTC")
      _other -> :invalid
    end
  end

  defp parse_integer(value) do
    case Integer.parse(value) do
      {integer, ""} -> integer
      _other -> :invalid
    end
  end

  defp parse_optional_integer(""), do: nil
  defp parse_optional_integer(value), do: parse_integer(value)

  defp mythic_result("", _player), do: "draw"

  defp mythic_result(winner, player),
    do: if(String.downcase(winner) == String.downcase(player), do: "win", else: "loss")

  defp required(errors, line, field, value) when value in [nil, "", :invalid],
    do: errors ++ [error(line, field, "is required and must be valid")]

  defp required(errors, _line, _field, _value), do: errors

  defp valid_positive(errors, line, field, value) when not is_integer(value) or value <= 0,
    do: errors ++ [error(line, field, "must be a positive integer")]

  defp valid_positive(errors, _line, _field, _value), do: errors

  defp valid_optional_positive(errors, _line, _field, nil), do: errors

  defp valid_optional_positive(errors, line, field, value),
    do: valid_positive(errors, line, field, value)

  defp valid_result(errors, line, value) when value not in ~w(win loss draw),
    do: errors ++ [error(line, "result", "must be win, loss, or draw")]

  defp valid_result(errors, _line, _value), do: errors

  defp max_length(errors, line, field, value, maximum)
       when is_binary(value) and byte_size(value) > maximum,
       do: errors ++ [error(line, field, "must be at most #{maximum} characters")]

  defp max_length(errors, _line, _field, _value, _maximum), do: errors

  defp valid_results?(seats) do
    winners = Enum.count(seats, &(&1.result == "win"))

    (winners == 1 and Enum.all?(seats, &(&1.result in ~w(win loss)))) or
      Enum.all?(seats, &(&1.result == "draw"))
  end

  defp duplicate?(items, mapper), do: items |> Enum.map(mapper) |> then(&(Enum.uniq(&1) != &1))

  defp consistent?(items, field),
    do: items |> Enum.map(&Map.fetch!(&1, field)) |> Enum.uniq() |> length() == 1

  defp add_game_error(errors, false, _lines, _field, _message), do: errors

  defp add_game_error(errors, true, lines, field, message),
    do: errors ++ Enum.map(lines, &error(&1, field, message))

  defp empty_error([], []), do: [error(1, "csv", "must include at least one data row")]
  defp empty_error(_games, _errors), do: []

  defp error(line, field, message), do: %{line: line, field: field, message: message}

  defp header_error(headers) do
    missing =
      @native_required
      |> Enum.reject(&(&1 in headers))
      |> Enum.map(&if(&1 == "gameid", do: "game_id", else: &1))

    "unrecognized CSV format; native format is missing: #{Enum.join(missing, ", ")}"
  end

  defp datetime_string(%DateTime{} = value), do: DateTime.to_iso8601(value)
  defp datetime_string(value), do: to_string(value)
end
