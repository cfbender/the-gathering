defmodule TheGathering.Discord.Tracker do
  @moduledoc "Keeps observed SpellBot reports in memory so `/won` can complete them."

  use GenServer

  alias TheGathering.Discord.{GameReport, Sink}

  def start_link(options) do
    GenServer.start_link(__MODULE__, options, name: __MODULE__)
  end

  @spec observe(GameReport.t()) :: :ok
  def observe(report), do: GenServer.call(__MODULE__, {:observe, report})

  @spec record_winner(String.t(), String.t()) :: {:ok, GameReport.t()} | {:error, atom()}
  def record_winner(game_id, discord_id) do
    GenServer.call(
      __MODULE__,
      {:record_winner, normalize_game_id(game_id), to_string(discord_id)}
    )
  end

  @impl true
  def init(options), do: {:ok, %{reports: %{}, sink: options[:sink]}}

  @impl true
  def handle_call({:observe, report}, _from, state) do
    :ok = dispatch(report, state.sink)
    {:reply, :ok, put_in(state.reports[report.external_id], report)}
  end

  def handle_call({:record_winner, external_id, discord_id}, _from, state) do
    with %GameReport{} = report <- state.reports[external_id],
         true <- Enum.any?(report.players, &(&1.discord_id == discord_id)) do
      completed = %GameReport{
        report
        | winner_discord_ids: [discord_id],
          raw: Map.put(report.raw, :winner_reported_by, discord_id)
      }

      :ok = dispatch(completed, state.sink)
      {:reply, {:ok, completed}, put_in(state.reports[external_id], completed)}
    else
      nil -> {:reply, {:error, :unknown_game}, state}
      false -> {:reply, {:error, :not_a_player}, state}
    end
  end

  defp dispatch(report, sink) do
    case Sink.dispatch(report, sink) do
      :ok -> :ok
      {:error, reason} -> exit({:sink_failed, reason})
    end
  end

  defp normalize_game_id(game_id) do
    game_id = game_id |> String.trim() |> String.upcase()
    game_id = if String.starts_with?(game_id, "SB"), do: game_id, else: "SB#{game_id}"
    "spellbot:#{game_id}"
  end
end
