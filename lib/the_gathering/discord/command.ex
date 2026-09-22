defmodule TheGathering.Discord.Command do
  @moduledoc false

  alias Nostrum.Api.ApplicationCommand
  alias TheGathering.Discord.{SummaryCommand, WonCommand}

  def definition do
    %{
      name: "won",
      description: "Report a SpellBot game's winner, kills, and game details",
      options: [
        %{
          type: 3,
          name: "game",
          description:
            "SpellBot game ID (e.g. SB12345). Defaults to the latest game in this channel",
          required: false
        }
      ]
    }
  end

  @doc """
  Registers `/won` and `/summary` for the application identified by the `READY` payload.

  Returns `{:ok, description}` for logging, or the Nostrum API error.
  """
  def register(application_id) do
    guild = guild_id()

    Enum.reduce_while(
      [definition(), SummaryCommand.definition()],
      {:ok,
       "registered /won and /summary #{if guild, do: "in guild #{guild}", else: "globally; new commands can take up to an hour to appear"}"},
      fn command, success ->
        result =
          if guild,
            do: ApplicationCommand.create_guild_command(application_id, guild, command),
            else: ApplicationCommand.create_global_command(application_id, command)

        case result do
          {:ok, _} -> {:cont, success}
          error -> {:halt, error}
        end
      end
    )
  end

  defp guild_id do
    case Application.get_env(:the_gathering, TheGathering.Discord, [])[:guild_id] do
      guild_id when is_binary(guild_id) and guild_id != "" -> guild_id
      _ -> nil
    end
  end

  defdelegate handle(interaction), to: WonCommand
end
