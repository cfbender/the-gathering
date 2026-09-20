defmodule TheGathering.Discord.Sink.Games do
  @moduledoc "Persists completed Discord game reports in the games domain."

  @behaviour TheGathering.Discord.Sink

  require Logger

  alias TheGathering.Discord.GameReport
  alias TheGathering.Games
  alias TheGathering.Repo

  @impl true
  def handle_report(%GameReport{} = report) do
    with :ok <- validate(report) do
      if report.winner_discord_ids == [] do
        Logger.info(
          "Discord game awaiting winner external_id=#{report.external_id} " <>
            "player_count=#{length(report.players)}"
        )

        :ok
      else
        persist(report)
      end
    end
  end

  defp persist(report) do
    case Repo.transaction(fn -> persist_report(report) end) do
      {:ok, {:ok, game}} ->
        Logger.info(
          "Discord game recorded external_id=#{report.external_id} game_id=#{game.id} " <>
            "player_count=#{length(report.players)}"
        )

        :ok

      {:error, reason} ->
        Logger.warning("Could not record Discord game external_id=#{report.external_id}")
        {:error, reason}
    end
  end

  defp persist_report(report) do
    case build_seats(report) do
      {:ok, seats} ->
        attrs = %{played_at: report.played_at, seats: seats}

        case Games.upsert_game_by_external_id("discord", report.external_id, attrs) do
          {:ok, game} -> {:ok, game}
          {:error, reason} -> Repo.rollback(reason)
        end

      {:error, reason} ->
        Repo.rollback(reason)
    end
  end

  defp build_seats(report) do
    winner_ids = MapSet.new(report.winner_discord_ids)

    report.players
    |> Enum.with_index(1)
    |> Enum.reduce_while({:ok, []}, fn {reported_player, seat_number}, {:ok, seats} ->
      with {:ok, player} <-
             Games.resolve_player(reported_player.display_name, reported_player.discord_id),
           {:ok, deck} <- find_or_create_deck(player, reported_player.commander_name) do
        seat = %{
          player_id: player.id,
          deck_id: deck && deck.id,
          seat: seat_number,
          result:
            if(MapSet.member?(winner_ids, reported_player.discord_id), do: "win", else: "loss")
        }

        {:cont, {:ok, [seat | seats]}}
      else
        {:error, reason} -> {:halt, {:error, reason}}
      end
    end)
    |> case do
      {:ok, seats} -> {:ok, Enum.reverse(seats)}
      error -> error
    end
  end

  defp find_or_create_deck(_player, commander_name)
       when commander_name in [nil, ""],
       do: {:ok, nil}

  defp find_or_create_deck(player, commander_name) do
    Games.find_or_create_deck(player, commander_name, %{commander_name: commander_name})
  end

  defp validate(report) do
    with :ok <- validate_identity(report),
         :ok <- validate_players(report.players) do
      validate_winners(report)
    end
  end

  defp validate_identity(report) do
    cond do
      report.source != "discord" ->
        {:error, :invalid_source}

      not is_binary(report.external_id) or report.external_id == "" ->
        {:error, :invalid_external_id}

      true ->
        :ok
    end
  end

  defp validate_players(players) do
    cond do
      length(players) not in 2..6 ->
        {:error, :invalid_player_count}

      Enum.any?(players, &invalid_player?/1) ->
        {:error, :invalid_player}

      duplicate_players?(players) ->
        {:error, :duplicate_player}

      true ->
        :ok
    end
  end

  defp validate_winners(report) do
    discord_ids = Enum.map(report.players, & &1.discord_id)

    cond do
      length(report.winner_discord_ids) > 1 ->
        {:error, :multiple_winners}

      Enum.any?(report.winner_discord_ids, &(&1 not in discord_ids)) ->
        {:error, :unknown_winner}

      true ->
        :ok
    end
  end

  defp duplicate_players?(players) do
    discord_ids = Enum.map(players, & &1.discord_id)
    Enum.uniq(discord_ids) != discord_ids
  end

  defp invalid_player?(player) do
    not is_binary(player.discord_id) or player.discord_id == "" or
      not is_binary(player.display_name) or String.trim(player.display_name) == ""
  end
end
