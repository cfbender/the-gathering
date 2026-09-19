defmodule TheGathering.Discord.Command do
  @moduledoc false

  alias Nostrum.Api.ApplicationCommand
  alias TheGathering.Discord.Tracker

  @ephemeral 64

  def definition do
    %{
      name: "won",
      description: "Record yourself as the winner of a SpellBot game",
      options: [
        %{
          type: 3,
          name: "game",
          description: "SpellBot game ID, for example SB12345",
          required: true
        }
      ]
    }
  end

  @doc """
  Registers `/won` for the application identified by the `READY` payload.

  Returns `{:ok, description}` for logging, or the Nostrum API error.
  """
  def register(application_id) do
    case guild_id() do
      nil ->
        with {:ok, _command} <-
               ApplicationCommand.create_global_command(application_id, definition()) do
          {:ok,
           "registered /won globally; Discord can take up to an hour to show new global commands"}
        end

      guild_id ->
        with {:ok, _command} <-
               ApplicationCommand.create_guild_command(application_id, guild_id, definition()) do
          {:ok, "registered /won in guild #{guild_id}"}
        end
    end
  end

  defp guild_id do
    case Application.get_env(:the_gathering, TheGathering.Discord, [])[:guild_id] do
      guild_id when is_binary(guild_id) and guild_id != "" -> guild_id
      _ -> nil
    end
  end

  def handle(%{data: %{name: "won", options: options}} = interaction) do
    game_id = option_value(options, "game")
    user = interaction.user || interaction.member.user

    content =
      case Tracker.record_winner(game_id, user.id) do
        {:ok, report} ->
          "Recorded you as the winner of #{String.replace_prefix(report.external_id, "spellbot:", "")}."

        {:error, :unknown_game} ->
          "I haven't seen that SpellBot game start. Check the game ID and make sure I can read the game channel."

        {:error, :not_a_player} ->
          "You weren't listed as a player in that SpellBot game, so I didn't change it."

        {:error, {:sink_failed, _reason}} ->
          "I couldn't save that game. Please try again or ask an administrator to check the logs."
      end

    response(content)
  end

  def handle(_interaction), do: response("I don't recognize that command.")

  defp option_value(options, name) do
    case Enum.find(options || [], &(&1.name == name)) do
      nil -> ""
      option -> option.value
    end
  end

  defp response(content), do: %{type: 4, data: %{content: content, flags: @ephemeral}}
end
