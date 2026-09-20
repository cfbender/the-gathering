defmodule TheGathering.Discord.Tracker do
  @moduledoc "Stages observed SpellBot reports in SQLite so `/won` can complete them."

  use GenServer

  alias TheGathering.Discord
  alias TheGathering.Discord.{GameReport, PendingGame, Sink}

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
  def init(options), do: {:ok, %{sink: options[:sink]}}

  @impl true
  def handle_call({:observe, report}, _from, state) do
    result =
      with :ok <- Sink.dispatch(report, state.sink),
           {:ok, _pending} <- Discord.stage_report(report) do
        Discord.prune_pending()
      end

    {:reply, result, state}
  end

  def handle_call({:record_winner, external_id, discord_id}, _from, state) do
    case Discord.get_pending_by_external_id(external_id) do
      %PendingGame{} = pending ->
        {:reply, Discord.resolve_pending(pending, discord_id, state.sink), state}

      nil ->
        {:reply, {:error, :unknown_game}, state}
    end
  end

  def handle_call({:record_latest_winner, channel_id, discord_id}, _from, state) do
    case Discord.latest_pending_in_channel(channel_id) do
      %PendingGame{} = pending ->
        {:reply, Discord.resolve_pending(pending, discord_id, state.sink), state}

      nil ->
        {:reply, {:error, :no_game_in_channel}, state}
    end
  end

  defp normalize_game_id(game_id) do
    game_id = game_id |> String.trim() |> String.upcase()
    game_id = if String.starts_with?(game_id, "SB"), do: game_id, else: "SB#{game_id}"
    "spellbot:#{game_id}"
  end
end
