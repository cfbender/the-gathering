defmodule TheGathering.Imports do
  @moduledoc """
  Imports game history from external data sources.

  Every source parses into the same game/seat shape, then `preview/2` matches
  players and decks against existing records and `import/3` commits the whole
  batch in one transaction. Games are keyed by `{source, external_id}`, so
  re-importing the same data skips games that already exist.
  """

  import Ecto.Query

  alias TheGathering.Games
  alias TheGathering.Games.{Deck, Game, Player}
  alias TheGathering.Imports.{CSV, MythicTrack}
  alias TheGathering.Repo

  @sources %{csv: "csv", mythic_track: "mythic_track"}
  @deck_attrs [
    :commander_card_id,
    :partner_name,
    :partner_card_id,
    :color_identity,
    :decklist_url
  ]

  def preview_csv(csv), do: preview(:csv, csv)
  def import_csv(csv, user_id), do: import(:csv, csv, user_id)

  def preview(source, payload) when is_map_key(@sources, source) and is_binary(payload) do
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

  def import(source, payload, user_id) when is_map_key(@sources, source) and is_binary(payload) do
    Repo.transaction(fn ->
      preview = preview(source, payload)

      if preview.valid do
        preview.games
        |> Enum.reduce(
          %{created: 0, skipped: 0, game_ids: []},
          &import_game(&1, &2, @sources[source], user_id)
        )
        |> Map.update!(:game_ids, &Enum.reverse/1)
      else
        Repo.rollback({:validation, preview})
      end
    end)
  end

  defp parse(:csv, csv) do
    case CSV.parse(csv) do
      {:ok, games, errors} -> {:ok, games, errors, []}
      {:error, errors} -> {:error, errors}
    end
  end

  defp parse(:mythic_track, json), do: MythicTrack.parse(json)

  defp import_game(game, result, source, user_id) do
    case Repo.get_by(Game, source: source, external_id: game.external_id) do
      %Game{id: id} ->
        %{result | skipped: result.skipped + 1, game_ids: [id | result.game_ids]}

      nil ->
        seats = Enum.map(game.seats, &import_seat/1)

        attrs = %{
          played_at: game.played_at,
          duration_minutes: game.duration_minutes,
          turns: game.turns,
          notes: game.notes,
          source: source,
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
    player = seat |> find_or_create_player() |> unwrap!()

    deck_attrs =
      seat
      |> Map.take(@deck_attrs)
      |> Map.reject(fn {_key, value} -> value in [nil, ""] end)
      |> Map.put(:commander_name, seat.commander)

    deck = player |> Games.find_or_create_deck(seat.deck, deck_attrs) |> unwrap!()

    %{
      player_id: player.id,
      deck_id: deck.id,
      seat: seat.seat,
      result: seat.result,
      mvp_card_name: seat.mvp_card
    }
  end

  # Prefer the Discord identity when the source supplies one, so a Mythic Track
  # player and the SpellBot/Discord-login player with the same account merge.
  # Otherwise match by name and remember the Discord ID for later logins.
  defp find_or_create_player(%{discord_id: discord_id} = seat) when is_binary(discord_id) do
    case Repo.get_by(Player, discord_id: discord_id) do
      %Player{} = player ->
        {:ok, player}

      nil ->
        with {:ok, player} <- Games.find_or_create_player_by_name(seat.player) do
          attach_discord_id(player, discord_id)
        end
    end
  end

  defp find_or_create_player(seat), do: Games.find_or_create_player_by_name(seat.player)

  defp attach_discord_id(%Player{discord_id: nil} = player, discord_id) do
    case Games.update_player(player, %{discord_id: discord_id}) do
      {:ok, player} -> {:ok, player}
      # Another player already owns this Discord ID; keep the name match.
      {:error, _changeset} -> {:ok, player}
    end
  end

  defp attach_discord_id(player, _discord_id), do: {:ok, player}

  defp match_players(games) do
    seats = all_seats(games)
    existing = existing_players(seats)

    seats
    |> Enum.uniq_by(&player_key/1)
    |> Enum.sort_by(&String.downcase(&1.player))
    |> Enum.reduce(%{create: [], matched: []}, fn seat, result ->
      case existing_player(existing, seat) do
        nil -> Map.update!(result, :create, &[seat.player | &1])
        player -> Map.update!(result, :matched, &[%{id: player.id, name: player.name} | &1])
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
      |> Map.new(fn deck -> {{deck.player_id, String.downcase(deck.name)}, deck} end)

    seats
    |> Enum.uniq_by(&{player_key(&1), String.downcase(&1.deck)})
    |> Enum.reduce(%{create: [], matched: []}, fn seat, result ->
      player = existing_player(players, seat)
      deck = player && existing[{player.id, String.downcase(seat.deck)}]

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

  defp player_key(seat), do: {:name, String.downcase(seat.player)}

  # Returns `%{by_name: %{lowered_name => player}, by_discord: %{discord_id => player}}`
  # for the players referenced by the seats.
  defp existing_players([]), do: %{by_name: %{}, by_discord: %{}}

  defp existing_players(seats) do
    names = seats |> Enum.map(&String.downcase(&1.player)) |> Enum.uniq()

    discord_ids =
      seats |> Enum.map(&Map.get(&1, :discord_id)) |> Enum.reject(&is_nil/1) |> Enum.uniq()

    players =
      Player
      |> where([player], fragment("lower(?)", player.name) in ^names)
      |> or_where([player], player.discord_id in ^discord_ids)
      |> Repo.all()

    %{
      by_name: Map.new(players, &{String.downcase(&1.name), &1}),
      by_discord: players |> Enum.reject(&is_nil(&1.discord_id)) |> Map.new(&{&1.discord_id, &1})
    }
  end

  defp existing_player(existing, seat) do
    case player_key(seat) do
      {:discord, discord_id} ->
        existing.by_discord[discord_id] || existing.by_name[String.downcase(seat.player)]

      {:name, name} ->
        existing.by_name[name]
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

  defp unwrap!({:ok, value}), do: value
  defp unwrap!({:error, changeset}), do: Repo.rollback({:changeset, changeset})
end
