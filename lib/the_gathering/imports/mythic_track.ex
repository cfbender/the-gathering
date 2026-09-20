defmodule TheGathering.Imports.MythicTrack do
  @moduledoc """
  Parses the JSON returned by Mythic Track's `POST api/games/get` endpoint.

  Mythic Track has no export feature, but its Blazor client fetches the signed-in
  user's full game list from that endpoint as a `List<GameViewModel>`. Users save
  that response and upload it here. Only completed games (`gameStatus == 3`) are
  imported; other statuses are reported as warnings.

  Produces the same game/seat shape as `TheGathering.Imports.CSV`, with `line`
  set to the game's 1-based position in the array. Seats additionally carry
  Discord IDs, Scryfall card IDs, colour identity, and decklist URLs when Mythic
  Track supplied them. A game's first key card becomes the winner's MVP card.
  """

  @status_complete 3
  @status_names %{1 => "not started", 2 => "in progress"}

  def parse(json) when is_binary(json) do
    case Jason.decode(json) do
      {:ok, games} when is_list(games) ->
        parse_games(games)

      {:ok, %{"data" => games}} when is_list(games) ->
        parse_games(games)

      {:ok, _other} ->
        {:error, [error(1, "json", "must be a JSON array of Mythic Track games")]}

      {:error, %Jason.DecodeError{} = exception} ->
        {:error, [error(1, "json", Exception.message(exception))]}
    end
  end

  defp parse_games([]), do: {:error, [error(1, "json", "must include at least one game")]}

  defp parse_games(games) do
    {parsed, errors, warnings} =
      games
      |> Enum.with_index(1)
      |> Enum.reduce({[], [], []}, fn {game, line}, {parsed, errors, warnings} ->
        case classify(game, line) do
          {:skip, warning} -> {parsed, errors, [warning | warnings]}
          {:ok, game} -> {[game | parsed], errors, warnings}
          {:error, game_errors} -> {parsed, errors ++ game_errors, warnings}
        end
      end)

    parsed = parsed |> Enum.reverse() |> Enum.sort_by(&{&1.played_at, &1.external_id})
    {:ok, parsed, errors, Enum.reverse(warnings)}
  end

  defp classify(game, line) when is_map(game) do
    case game["gameStatus"] do
      @status_complete ->
        build_game(game, line)

      status ->
        name = Map.get(@status_names, status, "status #{inspect(status)}")
        {:skip, %{line: line, message: "skipped: game is #{name}"}}
    end
  end

  defp classify(_game, line), do: {:error, [error(line, "game", "must be an object")]}

  defp build_game(game, line) do
    external_id = string(game["id"])
    played_at = parse_datetime(game["createdOn"])
    players = List.wrap(game["players"])

    key_cards = key_cards(game)

    seats =
      players
      |> Enum.with_index()
      |> Enum.sort_by(fn {player, index} -> {player["turnOrder"] || 1_000, index} end)
      |> Enum.with_index(1)
      |> Enum.map(fn {{player, _index}, seat} -> build_seat(player, seat, line, players) end)
      |> assign_key_cards(key_cards)

    errors =
      []
      |> required(line, "id", external_id)
      |> required(line, "createdOn", played_at)
      |> add_error(
        length(seats) not in 2..6,
        line,
        "players",
        "must contain between 2 and 6 players"
      )
      |> add_error(
        Enum.any?(seats, &(&1.player == "")),
        line,
        "players",
        "every player needs a name"
      )
      |> add_error(
        duplicate?(seats, &String.downcase(&1.player)),
        line,
        "players",
        "cannot contain the same player twice"
      )
      |> add_error(
        not valid_results?(seats),
        line,
        "isWinner",
        "must have at most one winner; games without a winner import as draws"
      )

    if errors == [] do
      {:ok,
       %{
         external_id: external_id,
         game_id: external_id,
         played_at: played_at,
         duration_minutes: positive_or_nil(game["gameTimeInMinutes"]),
         turns: positive_or_nil(game["totalTurns"]),
         notes: notes(game, key_cards, seats),
         lines: [line],
         seats: seats
       }}
    else
      {:error, errors}
    end
  end

  defp build_seat(player, seat, line, players) do
    commander = player["commander"] || %{}
    partner = player["commanderPartner"] || %{}
    commander_name = string(commander["name"])
    partner_name = blank_to_nil(string(partner["name"]))
    identity = player["player"] || %{}

    %{
      line: line,
      player: player_name(identity),
      discord_id: blank_to_nil(string(identity["discordUserId"])),
      deck: deck_name(commander, commander_name, partner_name),
      commander: commander_name,
      commander_card_id: blank_to_nil(string(commander["scryfallId"])),
      partner: partner_name,
      partner_card_id: blank_to_nil(string(partner["scryfallId"])),
      color_identity: color_identity(commander, partner),
      decklist_url: blank_to_nil(string(commander["decklistUrl"])),
      seat: seat,
      result: result(player, players),
      mvp_card: nil,
      mvp_card_id: nil
    }
  end

  # Mythic Track records a game's key cards without tying them to a seat; in
  # practice they are the cards that won the game, so the first one becomes the
  # winner's MVP. Remaining key cards are kept in the game notes.
  defp key_cards(game) do
    game["keyCards"]
    |> List.wrap()
    |> Enum.filter(&is_map/1)
    |> Enum.map(&%{name: string(&1["name"]), card_id: blank_to_nil(string(&1["scryfallId"]))})
    |> Enum.reject(&(&1.name == ""))
  end

  defp assign_key_cards(seats, []), do: seats

  defp assign_key_cards(seats, [mvp | _rest]) do
    Enum.map(seats, fn
      %{result: "win"} = seat -> %{seat | mvp_card: mvp.name, mvp_card_id: mvp.card_id}
      seat -> seat
    end)
  end

  # Key cards that did not become the winner's MVP (or all of them when the game
  # had no winner) are listed in the notes so the data is not lost.
  defp unassigned_key_cards(key_cards, seats) do
    if Enum.any?(seats, &(&1.result == "win")), do: Enum.drop(key_cards, 1), else: key_cards
  end

  defp player_name(identity) do
    [identity["name"], identity["friendlyName"], identity["username"]]
    |> Enum.map(&string/1)
    |> Enum.find("", &(&1 != ""))
  end

  defp deck_name(commander, commander_name, partner_name) do
    case string(commander["deckName"]) do
      "" when is_binary(partner_name) -> "#{commander_name} / #{partner_name}"
      "" -> commander_name
      name -> name
    end
  end

  defp color_identity(commander, partner) do
    colors = List.wrap(commander["colors"]) ++ List.wrap(partner["colors"])

    ~w(W U B R G)
    |> Enum.filter(fn color -> Enum.any?(colors, &(String.upcase(string(&1)) == color)) end)
    |> Enum.join()
  end

  defp result(player, players) do
    winners = Enum.count(players, &(&1["isWinner"] == true))

    cond do
      winners == 0 -> "draw"
      player["isWinner"] == true -> "win"
      true -> "loss"
    end
  end

  defp valid_results?(seats), do: Enum.count(seats, &(&1.result == "win")) <= 1

  defp notes(game, key_cards, seats) do
    extra_cards =
      case unassigned_key_cards(key_cards, seats) do
        [] -> ""
        cards -> "Key cards: " <> Enum.map_join(cards, ", ", & &1.name)
      end

    [game["name"], game["notes"], extra_cards]
    |> Enum.map(&string/1)
    |> Enum.reject(&(&1 == ""))
    |> Enum.uniq()
    |> case do
      [] -> nil
      parts -> Enum.join(parts, "\n")
    end
  end

  defp parse_datetime(value) when is_binary(value) do
    with {:error, _reason} <- DateTime.from_iso8601(value),
         {:ok, naive} <- NaiveDateTime.from_iso8601(value) do
      naive |> DateTime.from_naive!("Etc/UTC") |> DateTime.truncate(:second)
    else
      {:ok, datetime, _offset} -> DateTime.truncate(datetime, :second)
      _other -> :invalid
    end
  end

  defp parse_datetime(_value), do: :invalid

  defp positive_or_nil(value) when is_integer(value) and value > 0, do: value
  defp positive_or_nil(_value), do: nil

  defp string(nil), do: ""
  defp string(value) when is_binary(value), do: String.trim(value)
  defp string(value), do: value |> to_string() |> String.trim()

  defp blank_to_nil(""), do: nil
  defp blank_to_nil(value), do: value

  defp duplicate?(items, mapper), do: items |> Enum.map(mapper) |> then(&(Enum.uniq(&1) != &1))

  defp required(errors, line, field, value) when value in [nil, "", :invalid],
    do: errors ++ [error(line, field, "is required and must be valid")]

  defp required(errors, _line, _field, _value), do: errors

  defp add_error(errors, false, _line, _field, _message), do: errors
  defp add_error(errors, true, line, field, message), do: errors ++ [error(line, field, message)]

  defp error(line, field, message), do: %{line: line, field: field, message: message}
end
