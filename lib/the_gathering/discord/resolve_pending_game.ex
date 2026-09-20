defmodule TheGathering.Discord.ResolvePendingGame do
  @moduledoc "Resolves or discards a staged Discord game report."

  alias TheGathering.Discord.{GameReport, PendingGame, Sink}
  alias TheGathering.Repo

  def run(%PendingGame{} = pending, discord_id, sink) do
    report = to_report(pending)

    if Enum.any?(report.players, &(&1.discord_id == discord_id)) do
      completed = %GameReport{
        report
        | winner_discord_ids: [discord_id],
          raw: Map.put(report.raw, :winner_reported_by, discord_id)
      }

      case resolve_transaction(pending, completed, sink) do
        {:ok, :ok} -> {:ok, completed}
        {:error, reason} -> {:error, reason}
      end
    else
      {:error, :not_a_player}
    end
  end

  def discard(%PendingGame{} = pending), do: Repo.delete(pending)

  def to_report(%PendingGame{} = pending) do
    %GameReport{
      external_id: pending.external_id,
      source: "discord",
      played_at: pending.played_at,
      guild_id: pending.guild_id,
      channel_id: pending.channel_id,
      players: decode_players(pending.players["seats"]),
      winner_discord_ids: [],
      raw: pending.raw
    }
  end

  defp resolve_transaction(pending, completed, sink) do
    Repo.transaction(fn -> persist_and_consume(pending, completed, sink) end)
  end

  defp persist_and_consume(pending, completed, sink) do
    with :ok <- Sink.dispatch(completed, sink),
         {:ok, _pending} <- Repo.delete(pending) do
      :ok
    else
      {:error, reason} -> Repo.rollback({:sink_failed, reason})
    end
  end

  defp decode_players(players) do
    Enum.map(players, fn player ->
      %{
        discord_id: player["discord_id"],
        display_name: player["display_name"],
        commander_name: player["commander_name"]
      }
    end)
  end
end
