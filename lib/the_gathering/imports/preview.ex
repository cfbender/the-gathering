defmodule TheGathering.Imports.Preview do
  @moduledoc false

  import Ecto.Query

  alias TheGathering.Games
  alias TheGathering.Games.{Deck, Player}
  alias TheGathering.Imports.{CSV, MythicTrack}
  alias TheGathering.Repo

  def run(source, payload) do
    case parse(source, payload) do
      {:ok, games, errors, warnings} ->
        %{
          valid: errors == [],
          games: games,
          players: match_players(games),
          decks: match_decks(games),
          errors: errors,
          warnings: warnings
        }

      {:error, errors} ->
        %{
          valid: false,
          games: [],
          players: %{create: [], matched: []},
          decks: %{create: [], matched: []},
          errors: errors,
          warnings: []
        }
    end
  end

  defp parse(:csv, csv) do
    case CSV.parse(csv) do
      {:ok, games, errors} -> {:ok, games, errors, []}
      {:error, errors} -> {:error, errors}
    end
  end

  defp parse(:mythic_track, json), do: MythicTrack.parse(json)

  defp match_players(games) do
    seats = games |> all_seats() |> Enum.uniq_by(&player_key/1)

    Games.preview_player_resolutions(
      Enum.map(seats, &%{name: &1.player, discord_id: &1.discord_id})
    )
    |> Enum.reduce(%{create: [], matched: []}, fn resolution, result ->
      case resolution do
        %{status: :create, name: name} ->
          Map.update!(result, :create, &[name | &1])

        %{status: :matched, player: player} ->
          Map.update!(result, :matched, &[%{id: player.id, name: player.name} | &1])
      end
    end)
    |> then(fn result -> %{result | matched: Enum.uniq(result.matched)} end)
    |> sort_match_result()
  end

  defp match_decks(games) do
    seats = all_seats(games)
    players = existing_players(seats)

    existing =
      Deck
      |> preload(:player)
      |> Repo.all()
      |> Map.new(fn deck -> {{deck.player_id, Games.fold_name(deck.name)}, deck} end)

    seats
    |> Enum.uniq_by(&{player_key(&1), Games.fold_name(&1.deck)})
    |> Enum.reduce(%{create: [], matched: []}, fn seat, result ->
      player = existing_player(players, seat)
      deck = player && existing[{player.id, Games.fold_name(seat.deck)}]

      case deck do
        nil ->
          item = %{player: seat.player, name: seat.deck, commander: seat.commander}
          Map.update!(result, :create, &[item | &1])

        deck ->
          item = %{
            id: deck.id,
            player_id: player.id,
            player: player.name,
            name: deck.name,
            commander: deck.commander_name
          }

          Map.update!(result, :matched, &[item | &1])
      end
    end)
    |> then(fn result -> %{result | matched: Enum.uniq(result.matched)} end)
    |> sort_match_result()
  end

  defp all_seats(games), do: Enum.flat_map(games, & &1.seats)

  defp player_key(%{discord_id: discord_id}) when is_binary(discord_id),
    do: {:discord, discord_id}

  defp player_key(seat), do: {:name, Games.fold_name(seat.player)}

  defp existing_players([]), do: %{by_name: %{}, by_discord: %{}}

  defp existing_players(seats) do
    names = seats |> Enum.map(&Games.fold_name(&1.player)) |> Enum.uniq()
    discord_ids = seats |> Enum.map(& &1.discord_id) |> Enum.reject(&is_nil/1) |> Enum.uniq()

    players =
      Player
      |> where([player], fragment("lower(?)", player.name) in ^names)
      |> or_where([player], player.discord_id in ^discord_ids)
      |> Repo.all()

    %{
      by_name: Map.new(players, &{Games.fold_name(&1.name), &1}),
      by_discord: players |> Enum.reject(&is_nil(&1.discord_id)) |> Map.new(&{&1.discord_id, &1})
    }
  end

  defp existing_player(existing, seat) do
    case player_key(seat) do
      {:discord, discord_id} -> existing.by_discord[discord_id]
      {:name, name} -> existing.by_name[name]
    end
  end

  defp sort_match_result(result) do
    %{
      create: Enum.sort_by(result.create, &sort_value/1),
      matched: Enum.sort_by(result.matched, &sort_value/1)
    }
  end

  defp sort_value(value) when is_binary(value), do: String.downcase(value)
  defp sort_value(value), do: String.downcase(Map.get(value, :name, ""))
end
