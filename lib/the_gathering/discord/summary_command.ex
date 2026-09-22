defmodule TheGathering.Discord.SummaryCommand do
  @moduledoc false

  require Logger

  alias Nostrum.Api.Interaction
  alias TheGathering.{Accounts, Games}
  alias TheGathering.Accounts.User
  alias TheGathering.Discord.SummaryUpload
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

  def respond(interaction, api \\ Interaction, uploader \\ SummaryUpload) do
    Logger.info("Discord /summary invoked")

    case prepare(interaction) do
      {:ok, game} ->
        Logger.info("Discord /summary selected game #{game.id}")

        # Acknowledge before downloading art or rasterizing. Never retry a failed
        # acknowledgement: Discord may already have accepted it.
        with {:ok} <- api.create_response(interaction, %{type: 5}) |> log_response(:acknowledge) do
          response = render_response(game)
          Logger.info("Discord /summary upload started for game #{game.id}")

          uploader.edit_response(interaction, response)
          |> log_response(:upload)
        end

      {:error, reason} ->
        Logger.info("Discord /summary rejected: #{reason}")

        api.create_response(interaction, %{type: 4, data: %{content: message(reason), flags: 64}})
        |> log_response(:rejection)
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
    started = System.monotonic_time(:millisecond)
    Logger.info("Discord /summary rendering game #{game.id}")

    case Games.render_summary(game) do
      {:ok, png} ->
        Logger.info(
          "Discord /summary rendered game #{game.id} in #{System.monotonic_time(:millisecond) - started} ms (#{byte_size(png)} bytes)"
        )

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

  defp log_response({:error, reason} = result, stage) do
    Logger.error("Discord /summary #{stage} failed: #{failure_details(reason)}")
    result
  end

  defp log_response(result, stage) do
    Logger.info("Discord /summary #{stage} completed")
    result
  end

  # API error messages/bodies may include request data. Log only numeric codes,
  # never the interaction token, webhook URL, message content or raw response.
  defp failure_details(%Nostrum.Error.ApiError{status_code: status, response: response}) do
    code = if is_map(response), do: Map.get(response, :code, Map.get(response, "code"))
    status = if is_integer(status), do: status, else: "unknown"
    code = if is_integer(code), do: code, else: "unknown"
    "HTTP #{status}, Discord code #{code}"
  end

  defp failure_details(%Req.TransportError{reason: reason}) when is_atom(reason),
    do: Atom.to_string(reason)

  defp failure_details(reason) when is_atom(reason), do: Atom.to_string(reason)
  defp failure_details(_reason), do: "transport error"

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
