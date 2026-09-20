defmodule TheGathering.Discord do
  @moduledoc """
  Context for staged Discord reports and optional Discord gateway supervisor.

  The child is ignored unless `:bot_token` is configured at runtime. The token
  is consumed by Nostrum and is never retained in the client state or logged.
  """

  use Supervisor

  require Logger

  alias TheGathering.Discord.{GameReport, PendingGame, ResolvePendingGame, StageReport}
  alias TheGathering.Discord.Sink.Games, as: GamesSink
  alias TheGathering.Repo

  @pending_retention_days 30

  def stage_report(%GameReport{} = report), do: StageReport.run(report)

  def list_pending, do: Repo.all(PendingGame.ordered_query())

  def get_pending_by_external_id(external_id) do
    Repo.one(PendingGame.by_external_id_query(external_id))
  end

  def latest_pending_in_channel(channel_id) do
    Repo.one(PendingGame.latest_in_channel_query(channel_id))
  end

  def pending_report(%PendingGame{} = pending), do: ResolvePendingGame.to_report(pending)

  def resolve_pending(pending_or_id, discord_id, sink \\ GamesSink)

  def resolve_pending(%PendingGame{} = pending, discord_id, sink) do
    ResolvePendingGame.run(pending, to_string(discord_id), sink)
  end

  def resolve_pending(id, discord_id, sink) do
    case Repo.get(PendingGame, id) do
      %PendingGame{} = pending -> resolve_pending(pending, discord_id, sink)
      nil -> {:error, :unknown_game}
    end
  end

  def discard_pending(id) do
    case Repo.get(PendingGame, id) do
      %PendingGame{} = pending -> ResolvePendingGame.discard(pending)
      nil -> {:error, :unknown_game}
    end
  end

  def prune_pending(now \\ DateTime.utc_now()) do
    cutoff = DateTime.add(now, -@pending_retention_days, :day)
    {_count, nil} = Repo.delete_all(PendingGame.expired_query(cutoff))
    :ok
  end

  def child_spec(options) do
    config = Keyword.merge(Application.get_env(:the_gathering, __MODULE__, []), options)
    super(config)
  end

  def start_link(config) do
    if present?(config[:bot_token]) do
      Logger.info(
        "Discord bot enabled; connecting to the gateway (the application needs the Message Content intent)"
      )

      Supervisor.start_link(__MODULE__, config, name: __MODULE__)
    else
      Logger.info("Discord bot disabled: DISCORD_BOT_TOKEN is not set")
      :ignore
    end
  end

  @impl true
  def init(config) do
    case Application.ensure_all_started(:nostrum) do
      {:ok, _applications} ->
        sink =
          config[:sink] ||
            Application.get_env(:the_gathering, :discord_sink, TheGathering.Discord.Sink.Games)

        children = [
          {TheGathering.Discord.Tracker, sink: sink},
          TheGathering.Discord.Consumer
        ]

        Supervisor.init(children, strategy: :one_for_one)

      {:error, reason} ->
        # A bad token must not take the web app down with it. Nostrum's error
        # already names the cause (e.g. "Authentication rejected, invalid token").
        Logger.error(
          "Discord bot could not start; game tracking from Discord is off until the container restarts with a valid DISCORD_BOT_TOKEN: #{inspect(reason)}"
        )

        :ignore
    end
  end

  defp present?(value), do: is_binary(value) and value != ""
end
