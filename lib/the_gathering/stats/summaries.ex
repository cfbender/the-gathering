defmodule TheGathering.Stats.Summaries do
  @moduledoc "Response shaping shared by statistics views."

  alias TheGathering.Catalog
  alias TheGathering.Games.Deck
  alias TheGathering.Stats.Records

  def recent_game(game, tracked \\ nil) do
    winner = Enum.find(game.seats, &(&1.result == "win"))

    %{
      id: game.id,
      played_at: game.played_at,
      duration_minutes: game.duration_minutes,
      turns: game.turns,
      result: Records.tracked_result(tracked),
      winner: winner && entity(winner.player),
      players: length(game.seats)
    }
  end

  @duration_bin_minutes 15
  @turn_bin 2

  @doc """
  How long games run: duration and turn histograms plus the fastest win and longest
  game (as `recent_game/2` summaries, `nil` without timed games). `tracked_fun` picks
  the tracked seat(s) of a game; when it returns `nil` any winner counts as a win.
  """
  def game_lengths(games, tracked_fun \\ fn _game -> nil end) do
    timed = Enum.filter(games, & &1.duration_minutes)
    won = Enum.filter(timed, &won?(&1, tracked_fun.(&1)))

    %{
      durations: Records.histogram(Enum.map(games, & &1.duration_minutes), @duration_bin_minutes),
      turns: Records.histogram(Enum.map(games, & &1.turns), @turn_bin),
      fastest_win:
        won |> Enum.min_by(& &1.duration_minutes, fn -> nil end) |> maybe_recent_game(tracked_fun),
      longest_game:
        timed
        |> Enum.max_by(& &1.duration_minutes, fn -> nil end)
        |> maybe_recent_game(tracked_fun)
    }
  end

  defp won?(game, nil), do: Enum.any?(game.seats, &(&1.result == "win"))
  defp won?(_game, tracked), do: Records.tracked_result(tracked) == "win"

  defp maybe_recent_game(nil, _tracked_fun), do: nil
  defp maybe_recent_game(game, tracked_fun), do: recent_game(game, tracked_fun.(game))

  # A retired (archived) deck carries `retired: true` so the profile can fold it
  # away while its games keep counting; other entities omit the key.
  def entity(%Deck{} = deck),
    do: deck |> named_entity() |> maybe_put(:retired, if(deck.archived_at, do: true))

  def entity(value), do: named_entity(value)

  defp named_entity(%{id: id, name: name} = value) do
    %{id: id, name: name}
    |> maybe_put(:commander_name, Map.get(value, :commander_name))
    |> maybe_put(:art_crop_url, Map.get(value, :art_crop_url))
    |> maybe_put(:color_identity, Map.get(value, :color_identity))
  end

  def commander(key, card) do
    %{
      id: public_commander_id(key, card),
      name: card.name,
      art_crop_url: card.art_crop_url,
      color_identity: card.color_identity
    }
  end

  def deck(deck, card_summaries) do
    art = Catalog.card_summary(card_summaries, deck.commander_card_id, deck.commander_name)

    %{
      id: deck.id,
      name: deck.name,
      commander_name: deck.commander_name,
      color_identity: deck.color_identity,
      art_crop_url: art && art.art_crop_url
    }
  end

  def records(counts) do
    counts
    |> Enum.map(&Map.put(&1, :win_rate, Records.percentage(&1.wins, &1.games)))
    |> Enum.sort_by(&{-&1.games, -&1.win_rate, String.downcase(&1.name)})
  end

  defp public_commander_id({:id, id}, _card), do: id
  defp public_commander_id({:name, _normalized}, card), do: card.name

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
