defmodule TheGathering.Discord.StageReport do
  @moduledoc "Stages normalized Discord reports for later winner resolution."

  alias TheGathering.Discord.{GameReport, PendingGame}
  alias TheGathering.Repo

  @doc "Stages a report, replacing normalized data when SpellBot edits the same game."
  def run(%GameReport{} = report) do
    %PendingGame{}
    |> PendingGame.changeset(attrs(report))
    |> Repo.insert(
      conflict_target: :external_id,
      on_conflict: {:replace, [:guild_id, :channel_id, :played_at, :players, :raw, :updated_at]}
    )
  end

  defp attrs(report) do
    %{
      external_id: report.external_id,
      guild_id: report.guild_id,
      channel_id: report.channel_id,
      played_at: report.played_at,
      players: %{"seats" => encode_players(report.players)},
      raw: stringify_keys(report.raw)
    }
  end

  defp encode_players(players) do
    Enum.map(players, fn player ->
      %{
        "discord_id" => player.discord_id,
        "display_name" => player.display_name,
        "commander_name" => player.commander_name
      }
    end)
  end

  defp stringify_keys(value) when is_map(value) do
    Map.new(value, fn {key, nested} -> {to_string(key), stringify_keys(nested)} end)
  end

  defp stringify_keys(value) when is_list(value), do: Enum.map(value, &stringify_keys/1)
  defp stringify_keys(value), do: value
end
