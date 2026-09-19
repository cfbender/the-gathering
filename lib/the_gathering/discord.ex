defmodule TheGathering.Discord do
  @moduledoc """
  Optional Discord gateway client.

  The child is ignored unless `:bot_token` is configured at runtime. The token
  is consumed by Nostrum and is never retained in the client state or logged.
  """

  use Supervisor

  def child_spec(options) do
    config = Keyword.merge(Application.get_env(:the_gathering, __MODULE__, []), options)
    super(config)
  end

  def start_link(config) do
    if present?(config[:bot_token]) do
      Supervisor.start_link(__MODULE__, config, name: __MODULE__)
    else
      :ignore
    end
  end

  @impl true
  def init(config) do
    with {:ok, _applications} <- Application.ensure_all_started(:nostrum) do
      sink =
        config[:sink] ||
          Application.get_env(:the_gathering, :discord_sink, TheGathering.Discord.Sink.Games)

      children = [
        {TheGathering.Discord.Tracker, sink: sink},
        TheGathering.Discord.Consumer
      ]

      Supervisor.init(children, strategy: :one_for_one)
    end
  end

  defp present?(value), do: is_binary(value) and value != ""
end
