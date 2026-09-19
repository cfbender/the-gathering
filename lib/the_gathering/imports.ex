defmodule TheGathering.Imports do
  @moduledoc "Imports game history from external data sources."

  import Ecto.Query

  alias TheGathering.Games
  alias TheGathering.Games.{Deck, Game, Player}
  alias TheGathering.Imports.CSV
  alias TheGathering.Repo

  def preview_csv(csv) when is_binary(csv) do
    case CSV.parse(csv) do
      {:ok, games, errors} ->
        %{
          valid: errors == [],
          games: games,
          players: match_players(games),
          decks: match_decks(games),
          errors: errors
        }

      {:error, errors} ->
        %{
          valid: false,
          games: [],
          players: %{create: [], matched: []},
          decks: %{create: [], matched: []},
          errors: errors
        }
    end
  end

  def import_csv(csv, user_id) when is_binary(csv) do
    Repo.transaction(fn ->
      preview = preview_csv(csv)

      if preview.valid do
        preview.games
        |> Enum.reduce(%{created: 0, skipped: 0, game_ids: []}, &import_game(&1, &2, user_id))
        |> Map.update!(:game_ids, &Enum.reverse/1)
      else
        Repo.rollback({:validation, preview})
      end
    end)
  end

  defp import_game(game, result, user_id) do
    case Repo.get_by(Game, source: "csv", external_id: game.external_id) do
      %Game{id: id} ->
        %{result | skipped: result.skipped + 1, game_ids: [id | result.game_ids]}

      nil ->
        seats = Enum.map(game.seats, &import_seat/1)

        attrs = %{
          played_at: game.played_at,
          duration_minutes: game.duration_minutes,
          turns: game.turns,
          notes: game.notes,
          source: "csv",
          external_id: game.external_id,
          created_by_user_id: user_id,
          seats: seats
        }

        case Games.create_game(attrs) do
          {:ok, created} ->
            %{result | created: result.created + 1, game_ids: [created.id | result.game_ids]}

          {:error, changeset} ->
            Repo.rollback({:changeset, changeset})
        end
    end
  end

  defp import_seat(seat) do
    player = seat.player |> Games.find_or_create_player_by_name() |> unwrap!()

    deck =
      player
      |> Games.find_or_create_deck(seat.deck, %{commander_name: seat.commander})
      |> unwrap!()

    %{
      player_id: player.id,
      deck_id: deck.id,
      seat: seat.seat,
      result: seat.result,
      mvp_card_name: seat.mvp_card
    }
  end

  defp match_players(games) do
    names = games |> all_seats() |> Enum.map(& &1.player) |> unique_names()
    existing = existing_players(names)

    Enum.reduce(names, %{create: [], matched: []}, fn name, result ->
      key = String.downcase(name)

      case existing[key] do
        nil -> Map.update!(result, :create, &[name | &1])
        player -> Map.update!(result, :matched, &[%{id: player.id, name: player.name} | &1])
      end
    end)
    |> sort_match_result()
  end

  defp match_decks(games) do
    seats = all_seats(games)
    players = existing_players(Enum.map(seats, & &1.player))

    existing =
      Deck
      |> preload(:player)
      |> Repo.all()
      |> Map.new(fn deck ->
        player = deck.player
        {{String.downcase(player.name), String.downcase(deck.name)}, deck}
      end)

    seats
    |> Enum.uniq_by(&{String.downcase(&1.player), String.downcase(&1.deck)})
    |> Enum.reduce(%{create: [], matched: []}, fn seat, result ->
      key = {String.downcase(seat.player), String.downcase(seat.deck)}

      case existing[key] do
        nil ->
          item = %{player: seat.player, name: seat.deck, commander: seat.commander}
          Map.update!(result, :create, &[item | &1])

        deck ->
          player = players[String.downcase(seat.player)]

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
    |> sort_match_result()
  end

  defp all_seats(games), do: Enum.flat_map(games, & &1.seats)

  defp existing_players([]), do: %{}

  defp existing_players(names) do
    lowered = Enum.map(names, &String.downcase/1)

    Player
    |> where([player], fragment("lower(?)", player.name) in ^lowered)
    |> Repo.all()
    |> Map.new(&{String.downcase(&1.name), &1})
  end

  defp unique_names(names),
    do: names |> Enum.uniq_by(&String.downcase/1) |> Enum.sort_by(&String.downcase/1)

  defp sort_match_result(result) do
    %{
      create: Enum.sort_by(result.create, &sort_value/1),
      matched: Enum.sort_by(result.matched, &sort_value/1)
    }
  end

  defp sort_value(value) when is_binary(value), do: String.downcase(value)
  defp sort_value(value), do: String.downcase(Map.get(value, :name, ""))

  defp unwrap!({:ok, value}), do: value
  defp unwrap!({:error, changeset}), do: Repo.rollback({:changeset, changeset})
end
