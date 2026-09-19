defmodule TheGathering.Discord.Sink.Logger do
  @moduledoc "Default sink which records receipt without logging raw Discord data."

  @behaviour TheGathering.Discord.Sink

  require Logger

  @impl true
  def handle_report(report) do
    Logger.info(
      "Discord game report received external_id=#{report.external_id} " <>
        "player_count=#{length(report.players)} " <>
        "winner_count=#{length(report.winner_discord_ids)}"
    )

    :ok
  end
end
