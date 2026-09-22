defmodule TheGathering.Discord.SummaryUpload do
  @moduledoc false

  # Interaction webhooks authenticate with their short-lived token, not the bot
  # token. Bypass Nostrum's indefinitely waiting REST queue for this upload.
  def edit_response(interaction, response, request_options \\ []) do
    token = URI.encode(interaction.token, &URI.char_unreserved?/1)

    url =
      "https://discord.com/api/v10/webhooks/#{interaction.application_id}/#{token}/messages/@original"

    options =
      Keyword.merge(request_options,
        user_agent: "DiscordBot (https://github.com/cfbender/the-gathering, 0.1.0)",
        retry: false,
        redirect: false,
        finch: [conn_opts: [transport_opts: [timeout: 3_000]], pool_timeout: 3_000],
        request_timeout: 15_000,
        receive_timeout: 15_000
      )

    case Req.patch(url, Keyword.merge(options, body_options(response))) do
      {:ok, %{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %{status: status, body: body}} ->
        {:error, %Nostrum.Error.ApiError{status_code: status, response: body}}

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp body_options(%{files: [%{name: name, body: png}]} = response) do
    [
      form_multipart: [
        {"payload_json", Jason.encode!(Map.delete(response, :files))},
        {"files[0]", {png, filename: name, content_type: "image/png"}}
      ]
    ]
  end

  defp body_options(response), do: [json: response]
end
