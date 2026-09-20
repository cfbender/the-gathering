defmodule TheGatheringWeb.API.AdminDiscordPendingJSON do
  alias TheGathering.Discord
  alias TheGathering.Discord.PendingGame

  def index(%{pending_games: pending_games}) do
    %{data: Enum.map(pending_games, &data/1)}
  end

  defp data(%PendingGame{} = pending) do
    report = Discord.pending_report(pending)

    %{
      id: pending.id,
      external_id: pending.external_id,
      guild_id: pending.guild_id,
      channel_id: pending.channel_id,
      played_at: pending.played_at,
      players: report.players,
      inserted_at: pending.inserted_at,
      updated_at: pending.updated_at
    }
  end
end
