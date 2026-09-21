defmodule TheGathering.Imports.SheetMatch do
  @moduledoc false
  alias TheGathering.Games

  def find(row, seats, candidates, decks) do
    ids = Enum.map(seats, & &1.player_id)

    matches =
      if nil in ids or Enum.uniq(ids) != ids do
        []
      else
        Enum.filter(candidates, fn game ->
          Enum.sort(ids) == Enum.sort(Enum.map(game.seats, & &1.player_id))
        end)
      end

    same_day = Enum.filter(matches, &(DateTime.to_date(&1.played_at) == row.date))
    pool = if same_day == [], do: matches, else: same_day
    date_reason = if same_day == [], do: "nearby date", else: "date"

    case pool do
      [game] -> {game, "Matched by #{date_reason} and players"}
      [] -> {nil, "No match with the same players on nearby dates"}
      games -> distinguish(games, seats, decks, date_reason)
    end
  end

  defp distinguish(games, seats, decks, date_reason) do
    ranked =
      games
      |> Enum.map(&{&1, deck_score(&1, seats, decks)})
      |> Enum.sort_by(&elem(&1, 1), :desc)

    case ranked do
      [{game, score}, {_, next} | _] when score > next and score > 0 ->
        {game, "Matched by #{date_reason}, players and decks"}

      _ ->
        {nil, "Multiple games match these players and dates; choose a game"}
    end
  end

  defp deck_score(game, seats, decks) do
    Enum.count(seats, fn seat ->
      existing = Enum.find(game.seats, &(&1.player_id == seat.player_id))
      deck = Enum.find(decks, &(&1.id == existing.deck_id))

      deck &&
        (seat.deck_id == deck.id or same_name?(seat.deck, deck.name) or
           same_name?(seat.deck, deck.commander_name))
    end)
  end

  # Short commander names can identify a deck, but arbitrary fuzzy nicknames cannot.
  defp same_name?(left, right) do
    left = normalize(left)
    right = normalize(right)
    left == right or (String.length(left) >= 4 and String.starts_with?(right, left <> " "))
  end

  defp normalize(name),
    do: name |> Games.fold_name() |> String.replace(~r/[^\p{L}\p{N}]+/u, " ") |> String.trim()
end
