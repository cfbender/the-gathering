NimbleCSV.define(TheGathering.Imports.GoogleSheetCSVParser, separator: ",", escape: "\"")
NimbleCSV.define(TheGathering.Imports.GoogleSheetTSVParser, separator: "\t", escape: "\"")

defmodule TheGathering.Imports.GoogleSheet do
  @moduledoc "Parses game rows exported from the group's Google Sheet."

  alias TheGathering.Imports.{GoogleSheetCSVParser, GoogleSheetTSVParser}

  @columns ["date", "winner", "deck", "win con", "other decks", "notes"]
  @draw_warning "No winner: all listed players will be recorded as a draw. Notes do not change results."
  @pair ~r/(?<player>[^(),;]+?)\s*\((?<deck>[^()]*)\)/

  @spec parse(binary()) :: {:ok, [map()]} | {:error, String.t()}
  def parse(payload) when is_binary(payload) do
    parser =
      if String.contains?(payload, "\t"), do: GoogleSheetTSVParser, else: GoogleSheetCSVParser

    with {:ok, parsed} <- parse_file(parser, payload),
         {header, data, header_line} <- find_header(parsed),
         {:ok, indexes} <- validate_header(header) do
      rows =
        data
        |> Enum.with_index(header_line + 1)
        |> Enum.reject(fn {row, _line} -> Enum.all?(row, &(String.trim(&1) == "")) end)
        |> Enum.map(fn {row, line} -> build_row(row, line, indexes) end)
        |> add_occurrences()

      {:ok, rows}
    else
      :no_header -> {:error, "Could not find the Google Sheet header row."}
      {:error, message} -> {:error, message}
    end
  end

  def parse(_payload), do: {:error, "File contents must be text."}

  defp parse_file(parser, payload) do
    {:ok, parser.parse_string(payload, skip_headers: false)}
  rescue
    exception in NimbleCSV.ParseError ->
      # Plain pasted cells can contain literal quotes, unlike a quoted TSV export.
      # Never use this fallback for a file that began a quoted field: that could
      # silently split a malformed multiline cell into separate games.
      if parser == GoogleSheetTSVParser and not Regex.match?(~r/(?:^|[\t\r\n])"/, payload) do
        {:ok, payload |> String.split(~r/\r?\n/) |> Enum.map(&String.split(&1, "\t"))}
      else
        {:error, "Could not parse file: #{Exception.message(exception)}"}
      end
  end

  defp find_header(rows) do
    case Enum.find_index(rows, &header_row?/1) do
      nil -> :no_header
      index -> {Enum.at(rows, index), Enum.drop(rows, index + 1), index + 1}
    end
  end

  defp header_row?(row) do
    normalized = Enum.map(row, &normalize_header/1)
    "date" in normalized and "winner" in normalized and "deck" in normalized
  end

  defp validate_header(header) do
    normalized = Enum.map(header, &normalize_header/1)
    missing = Enum.reject(@columns, &(&1 in normalized))

    cond do
      missing != [] ->
        {:error, "Google Sheet header is missing: #{Enum.join(missing, ", ")}."}

      position(normalized, "deck") >= position(normalized, "win con") ->
        {:error, "Google Sheet kill columns must be between Deck and Win Con."}

      true ->
        {:ok,
         %{
           date: position(normalized, "date"),
           winner: position(normalized, "winner"),
           deck: position(normalized, "deck"),
           win_con: position(normalized, "win con"),
           other_decks: position(normalized, "other decks"),
           notes: position(normalized, "notes"),
           kills: kill_columns(header, normalized)
         }}
    end
  end

  defp kill_columns(header, normalized) do
    first = position(normalized, "deck") + 1
    last = position(normalized, "win con") - 1

    if first <= last do
      first..last
      |> Enum.map(&{&1, String.trim(Enum.at(header, &1, ""))})
      |> Enum.reject(fn {_index, name} -> name == "" end)
    else
      []
    end
  end

  defp build_row(raw, line, indexes) do
    winner = value(raw, indexes.winner)
    winner_deck = value(raw, indexes.deck)
    {opponents, opponent_errors} = parse_opponents(value(raw, indexes.other_decks))
    draw? = winner == "" or String.downcase(winner) == "n/a"

    participants =
      if draw?, do: opponents, else: [%{player: winner, deck: winner_deck} | opponents]

    {kill_counts, kill_errors} = parse_kills(raw, indexes.kills)
    total_kills = Enum.sum(Enum.map(kill_counts, & &1.kills))

    errors =
      opponent_errors
      |> add_error(not draw? and winner_deck == "", "Winner is missing a deck.")
      |> add_error(
        length(participants) not in 2..6,
        "Game must have between 2 and 6 players; Other Decks cannot be missing."
      )
      |> add_error(duplicate_players?(participants), "A raw player is listed more than once.")
      |> Kernel.++(kill_errors)
      |> add_error(
        total_kills > max(length(participants) - 1, 0),
        "Total recorded kills exceed participants minus one."
      )

    seats =
      Enum.map(participants, fn participant ->
        %{
          player: participant.player,
          deck: participant.deck,
          kills: exact_kills(kill_counts, participant.player),
          result: result(draw?, winner, participant.player)
        }
      end)

    %{
      key: raw_key({:blank_kills_zero, indexes, raw}),
      line: line,
      date: parse_date(value(raw, indexes.date)),
      winner: winner,
      deck: winner_deck,
      win_con: value(raw, indexes.win_con),
      notes: value(raw, indexes.notes),
      seats: seats,
      kill_counts: kill_counts,
      errors: add_date_error(errors, value(raw, indexes.date)),
      warnings: if(draw?, do: [@draw_warning], else: [])
    }
  end

  defp parse_opponents(""), do: {[], []}

  defp parse_opponents(text) do
    pairs =
      Regex.scan(@pair, text, capture: :all_names)
      |> Enum.map(fn [deck, player] ->
        %{player: String.trim(player), deck: String.trim(deck)}
      end)

    residue = @pair |> Regex.replace(text, "") |> String.replace(~r/[\s,;]+/, "")
    errors = if residue == "", do: [], else: ["Other Decks contains malformed text: #{text}"]

    {pairs,
     add_error(
       errors,
       Enum.any?(pairs, &(&1.player == "" or &1.deck == "")),
       "Every opponent needs a player and deck name."
     )}
  end

  defp parse_kills(row, columns) do
    Enum.reduce(columns, {[], []}, fn {index, player}, {counts, errors} ->
      case parse_kill(value(row, index)) do
        :invalid -> {counts, errors ++ ["Kills for #{player} must be a nonnegative integer."]}
        kills -> {counts ++ [%{player: player, kills: kills}], errors}
      end
    end)
  end

  defp parse_kill(""), do: 0

  defp parse_kill(raw) do
    case Integer.parse(raw) do
      {kills, ""} when kills >= 0 -> kills
      _ -> :invalid
    end
  end

  defp exact_kills(counts, player) do
    case Enum.find(counts, &(String.downcase(&1.player) == String.downcase(player))) do
      nil -> 0
      count -> count.kills
    end
  end

  defp result(true, _winner, _player), do: "draw"

  defp result(false, winner, player),
    do: if(String.downcase(winner) == String.downcase(player), do: "win", else: "loss")

  defp parse_date(value) do
    with {:error, _} <- Date.from_iso8601(value),
         [month, day, year] <- String.split(value, "/"),
         {month, ""} <- Integer.parse(month),
         {day, ""} <- Integer.parse(day),
         {year, ""} <- Integer.parse(year),
         year = if(year < 100, do: 2000 + year, else: year),
         {:ok, date} <- Date.new(year, month, day) do
      date
    else
      {:ok, date} -> date
      _ -> nil
    end
  end

  defp add_date_error(errors, value),
    do: add_error(errors, is_nil(parse_date(value)), "Date is invalid.")

  defp duplicate_players?(participants) do
    names = Enum.map(participants, &(String.downcase(&1.player) |> String.trim()))
    Enum.uniq(names) != names
  end

  defp add_error(errors, true, message), do: errors ++ [message]
  defp add_error(errors, false, _message), do: errors

  defp add_occurrences(rows) do
    rows
    |> Enum.map_reduce(%{}, fn row, counts ->
      occurrence = Map.get(counts, row.key, 0) + 1
      counts = Map.put(counts, row.key, occurrence)

      if occurrence == 1 do
        {row, counts}
      else
        {%{
           row
           | key: "#{row.key}-#{occurrence}",
             warnings: row.warnings ++ ["Duplicate row occurrence #{occurrence}."]
         }, counts}
      end
    end)
    |> elem(0)
  end

  defp raw_key(row),
    do: :crypto.hash(:sha256, :erlang.term_to_binary(row)) |> Base.encode16(case: :lower)

  defp value(row, index), do: row |> Enum.at(index, "") |> String.trim()
  defp position(headers, name), do: Enum.find_index(headers, &(&1 == name))

  defp normalize_header(header),
    do: header |> String.trim() |> String.downcase() |> String.replace(~r/\s+/, " ")
end
