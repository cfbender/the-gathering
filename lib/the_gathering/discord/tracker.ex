defmodule TheGathering.Discord.Tracker do
  @moduledoc "Keeps observed SpellBot reports in memory so `/won` can complete them."

  use GenServer

  alias TheGathering.Discord.{GameReport, Sink}

  def start_link(options) do
    GenServer.start_link(__MODULE__, options, name: __MODULE__)
  end

  @spec observe(GameReport.t()) :: :ok | {:error, term()}
  def observe(report), do: GenServer.call(__MODULE__, {:observe, report})

  @spec record_winner(String.t(), String.t()) :: {:ok, GameReport.t()} | {:error, term()}
  def record_winner(game_id, discord_id) do
    GenServer.call(
      __MODULE__,
      {:record_winner, normalize_game_id(game_id), to_string(discord_id)}
    )
  end

  @doc """
  Records `discord_id` as the winner of the most recently started game observed
  in `channel_id`, for `/won` invoked without an explicit game ID.
  """
  @spec record_latest_winner(String.t(), String.t()) :: {:ok, GameReport.t()} | {:error, term()}
  def record_latest_winner(channel_id, discord_id) do
    GenServer.call(
      __MODULE__,
      {:record_latest_winner, to_string(channel_id), to_string(discord_id)}
    )
  end

  @impl true
  def init(options), do: {:ok, %{reports: %{}, sink: options[:sink]}}

  @impl true
  def handle_call({:observe, report}, _from, state) do
    case Sink.dispatch(report, state.sink) do
      :ok -> {:reply, :ok, put_in(state.reports[report.external_id], report)}
      {:error, reason} -> {:reply, {:error, reason}, state}
    end
  end

  def handle_call({:record_winner, external_id, discord_id}, _from, state) do
    case state.reports[external_id] do
      %GameReport{} = report -> complete(report, discord_id, state)
      nil -> {:reply, {:error, :unknown_game}, state}
    end
  end

  def handle_call({:record_latest_winner, channel_id, discord_id}, _from, state) do
    case latest_in_channel(state.reports, channel_id) do
      %GameReport{} = report -> complete(report, discord_id, state)
      nil -> {:reply, {:error, :no_game_in_channel}, state}
    end
  end

  defp complete(%GameReport{} = report, discord_id, state) do
    if Enum.any?(report.players, &(&1.discord_id == discord_id)) do
      completed = %GameReport{
        report
        | winner_discord_ids: [discord_id],
          raw: Map.put(report.raw, :winner_reported_by, discord_id)
      }

      case Sink.dispatch(completed, state.sink) do
        :ok ->
          {:reply, {:ok, completed}, put_in(state.reports[report.external_id], completed)}

        {:error, reason} ->
          {:reply, {:error, {:sink_failed, reason}}, state}
      end
    else
      {:reply, {:error, :not_a_player}, state}
    end
  end

  # SpellBot's ready embed carries the game's start time, so the latest
  # `played_at` is the most recently started game even when an older post is
  # edited (and re-observed) after a newer game began.
  defp latest_in_channel(reports, channel_id) do
    reports
    |> Map.values()
    |> Enum.filter(&(&1.channel_id == channel_id))
    |> Enum.max_by(& &1.played_at, DateTime, fn -> nil end)
  end

  defp normalize_game_id(game_id) do
    game_id = game_id |> String.trim() |> String.upcase()
    game_id = if String.starts_with?(game_id, "SB"), do: game_id, else: "SB#{game_id}"
    "spellbot:#{game_id}"
  end
end
