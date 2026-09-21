defmodule TheGathering.Discord.SummaryCommand do
  @moduledoc false

  require Logger

  alias Nostrum.Api.Interaction
  alias TheGathering.{Accounts, Games}
  alias TheGathering.Accounts.User
  alias TheGathering.Games.SummaryCard
  alias TheGatheringWeb.Endpoint

  def definition do
    %{
      name: "summary",
      description: "Post a rendered recap of a recorded game (defaults to the latest game)",
      dm_permission: false,
      options: [
        %{
          type: 3,
          name: "game",
          description: "Gathering game ID (123) or SpellBot ID (SB12345)",
          required: false
        }
      ]
    }
  end

  def respond(interaction, api \\ Interaction) do
    case prepare(interaction) do
      {:ok, game} ->
        # Acknowledge before downloading art or rasterizing. Never retry a failed
        # acknowledgement: Discord may already have accepted it.
        with :ok <- api.create_response(interaction, %{type: 5}) do
          api.edit_response(interaction, render_response(game))
        end

      {:error, reason} ->
        api.create_response(interaction, %{type: 4, data: %{content: message(reason), flags: 64}})
    end
  end

  def prepare(interaction) do
    user = Map.get(interaction, :user)
    guild = Map.get(interaction, :guild_id)
    configured = Application.get_env(:the_gathering, TheGathering.Discord, [])[:guild_id]

    with true <-
           not is_nil(guild) and
             (configured in [nil, ""] or to_string(guild) == to_string(configured)),
         %{id: discord_id} <- user,
         %User{disabled_at: nil} <- Accounts.get_user_by_discord_id(to_string(discord_id)) do
      options = Map.get(interaction.data, :options) || []
      option = Enum.find(options, &(&1.name == "game"))
      Games.find_summary_game(if option, do: option.value, else: "")
    else
      _ -> {:error, :forbidden}
    end
  end

  def render_response(game) do
    case Games.render_summary(game) do
      {:ok, png} ->
        name = "game-#{game.id}-summary.png"

        %{
          content: "Game ##{game.id} · <#{Endpoint.url()}/games/#{game.id}>",
          allowed_mentions: %{parse: []},
          attachments: [%{id: 0, filename: name, description: SummaryCard.description(game)}],
          files: [%{name: name, body: png}]
        }

      {:error, reason} ->
        Logger.warning("Discord summary rendering failed for game #{game.id}: #{inspect(reason)}")
        %{content: message(reason), allowed_mentions: %{parse: []}}
    end
  end

  defp message(:forbidden),
    do:
      "Sign in to The Gathering with Discord first. Summaries are only available to active members in the bot's server."

  defp message(:not_found),
    do: "No recorded game found. SpellBot games must have a recorded result first (use `/won`)."

  defp message(:bad_request),
    do: "Use a Gathering game ID such as `123`, or a SpellBot ID such as `SB12345`."

  defp message(:rate_limited), do: "Too many summaries requested. Please try again in a minute."

  defp message(_reason),
    do:
      "I couldn't render that summary. Please try again or ask an administrator to check the renderer."
end
