defmodule TheGathering.Discord.Consumer do
  @moduledoc false

  use Nostrum.Consumer

  require Logger

  alias Nostrum.Api.{Interaction, Self}
  alias TheGathering.Discord.{Command, SpellBotParser, Tracker}

  # Discord activity type 3 renders as "Watching …" under the bot's name.
  @watching 3

  def handle_event({:READY, ready, ws_state}) do
    Logger.info(
      "Discord bot connected as #{ready.user.username} in #{length(ready.guilds)} guild(s)"
    )

    # Nostrum identifies without a presence, so announce one on this shard's
    # session to show the bot as online and describe what it is doing.
    Self.update_shard_status(ws_state.conn_pid, :online, "SpellBot games", @watching)

    case Command.register(ready.application.id) do
      {:ok, description} -> Logger.info("Discord #{description}")
      error -> Logger.error("Could not register the Discord /won command: #{inspect(error)}")
    end
  end

  def handle_event({:MESSAGE_CREATE, message, _ws_state}), do: observe(message, "new")

  def handle_event({:MESSAGE_UPDATE, {_old_message, message}, _ws_state}),
    do: observe(message, "edited")

  def handle_event({:INTERACTION_CREATE, %{data: %{name: "won"}} = interaction, _ws_state}) do
    Logger.info("Discord /won invoked by user #{interaction_user_id(interaction)}")

    case Interaction.create_response(interaction, Command.handle(interaction)) do
      {:ok} -> :ok
      error -> Logger.error("Could not respond to the Discord /won command: #{inspect(error)}")
    end
  end

  # Logs describe the game by SpellBot ID and player count only; message text is never logged.
  defp observe(message, kind) do
    case SpellBotParser.parse(message) do
      {:ok, report} ->
        case Tracker.observe(report) do
          :ok ->
            Logger.info(
              "Discord observed SpellBot game #{report.external_id} with #{length(report.players)} player(s)"
            )

          {:error, reason} ->
            Logger.warning(
              "Could not record Discord game #{report.external_id}: #{inspect(reason)}"
            )
        end

      # Every other message on the server arrives here; only SpellBot's are worth a line.
      {:error, :not_spellbot} ->
        :ignore

      # SpellBot defers every /lfg and /game interaction, so its first message is
      # an empty "thinking" placeholder, and its validation replies are plain
      # text. The ready embed arrives later as an edit of the waiting post.
      # Stripped embeds are not a possibility here: the bot requests the Message
      # Content intent on connect, and Discord refuses the connection (close 4014)
      # instead of delivering empty messages when the intent is not enabled.
      {:error, reason} ->
        Logger.debug(
          "Discord ignored a #{kind} SpellBot message (type #{inspect(Map.get(message, :type))}): #{inspect(reason)}"
        )
    end
  end

  defp interaction_user_id(%{user: %{id: id}}) when not is_nil(id), do: id
  defp interaction_user_id(%{member: %{user: %{id: id}}}), do: id
  defp interaction_user_id(_interaction), do: "unknown"
end
