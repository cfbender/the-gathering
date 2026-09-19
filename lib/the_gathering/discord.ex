defmodule TheGathering.Discord do
  @moduledoc """
  Optional Discord gateway client.

  The child is ignored unless `:bot_token` is configured at runtime. The token
  is consumed by Nostrum and is never retained in the client state or logged.
  """

  use Supervisor

  require Logger

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
