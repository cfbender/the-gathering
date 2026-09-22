defmodule TheGathering.Discord.LogCommand do
  @moduledoc false

  require Logger

  alias Nostrum.Api.Interaction
  alias TheGathering.Discord.WebGameDraft
  alias TheGatheringWeb.Endpoint

  def respond(interaction, api \\ Interaction) do
    case api.create_response(interaction, handle(interaction)) do
      {:ok} -> :ok
      {:error, _} -> Logger.error("Discord /log response failed")
    end
  end

  def handle(interaction) do
    actor = %{
      discord_id: to_string(interaction.user && interaction.user.id),
      guild_id: to_string(interaction.guild_id),
      channel_id: to_string(interaction.channel_id)
    }

    options = interaction.data.options || []
    reference = option(options, "game", "") |> String.trim()
    winner = option(options, "winner", nil)
    winner = if winner, do: to_string(winner)

    case WebGameDraft.open(reference, winner, actor) do
      {:ok, draft} ->
        response(
          "Finish logging this game in The Gathering. Sign in with the same Discord account. Nothing is saved until you submit; this link expires in one hour.",
          [
            %{
              type: 1,
              components: [
                %{
                  type: 2,
                  style: 5,
                  label: "Open game log",
                  url: Endpoint.url() <> "/games/new?discord=" <> draft.id
                }
              ]
            }
          ]
        )

      {:error, :invalid_winner} ->
        response("Choose a winner from this SpellBot game's players.")

      {:error, :not_found} ->
        response("No unfinished game found. Try /log game:SB12345.")

      {:error, _} ->
        response("Use /log in the game's server. Disabled accounts cannot log games.")
    end
  end

  defp option(options, name, default) do
    case Enum.find(options, &(&1.name == name)) do
      nil -> default
      option -> option.value
    end
  end

  defp response(content, components \\ []),
    do: %{
      type: 4,
      data: %{content: content, components: components, flags: 64, allowed_mentions: %{parse: []}}
    }
end
