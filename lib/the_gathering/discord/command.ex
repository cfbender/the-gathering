defmodule TheGathering.Discord.Command do
  @moduledoc false

  alias Nostrum.Api.ApplicationCommand
  alias TheGathering.Discord.{SummaryCommand, WonCommand}

  def definition do
    %{
      name: "log",
      description: "Open a prefilled game log for a SpellBot game",
      dm_permission: false,
      options: [
        %{
          type: 6,
          name: "winner",
          description: "Optional winner; otherwise choose in the game log",
          required: false
        },
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
  Registers `/log` and `/summary`, removing only the replaced `/won` command.

  Returns `{:ok, description}` for logging, or the Nostrum API error.
  """
  def register(application_id, api \\ ApplicationCommand) do
    guild = guild_id()

    result =
      Enum.reduce_while(
        [definition(), SummaryCommand.definition()],
        {:ok,
         "registered /log and /summary #{if guild, do: "in guild #{guild}", else: "globally; new commands can take up to an hour to appear"}"},
        fn command, success ->
          result =
            if guild,
              do: api.create_guild_command(application_id, guild, command),
              else: api.create_global_command(application_id, command)

          case result do
            {:ok, _} -> {:cont, success}
            error -> {:halt, error}
          end
        end
      )

    with {:ok, description} <- result,
         {:ok} <- remove_legacy_command(application_id, guild, api) do
      {:ok, description}
    end
  end

  defp remove_legacy_command(application_id, guild, api) do
    commands =
      if guild,
        do: api.guild_commands(application_id, guild),
        else: api.global_commands(application_id)

    with {:ok, commands} <- commands do
      delete_legacy_command(Enum.find(commands, &(&1.name == "won")), application_id, guild, api)
    end
  end

  defp delete_legacy_command(nil, _application_id, _guild, _api), do: {:ok}

  defp delete_legacy_command(command, application_id, nil, api),
    do: api.delete_global_command(application_id, command.id)

  defp delete_legacy_command(command, application_id, guild, api),
    do: api.delete_guild_command(application_id, guild, command.id)

  defp guild_id do
    case Application.get_env(:the_gathering, TheGathering.Discord, [])[:guild_id] do
      guild_id when is_binary(guild_id) and guild_id != "" -> guild_id
      _ -> nil
    end
  end

  defdelegate handle(interaction), to: WonCommand
end
