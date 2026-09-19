defmodule TheGathering.Discord.Consumer do
  @moduledoc false

  use Nostrum.Consumer

  require Logger

  alias Nostrum.Api.Interaction
  alias TheGathering.Discord.{Command, SpellBotParser, Tracker}

  def handle_event({:READY, _ready, _ws_state}) do
    case Command.register() do
      {:ok, _command} -> :ok
      error -> Logger.error("Could not register Discord command: #{inspect(error)}")
    end
  end

  def handle_event({:MESSAGE_CREATE, message, _ws_state}), do: observe(message)

  def handle_event({:MESSAGE_UPDATE, {_old_message, message}, _ws_state}), do: observe(message)

  def handle_event({:INTERACTION_CREATE, %{data: %{name: "won"}} = interaction, _ws_state}) do
    Interaction.create_response(interaction, Command.handle(interaction))
  end

  defp observe(message) do
    case SpellBotParser.parse(message) do
      {:ok, report} -> Tracker.observe(report)
      {:error, _reason} -> :ignore
    end
  end
end
