defmodule TheGathering.CloudflareTurn do
  @moduledoc """
  Mints short-lived Cloudflare Realtime TURN credentials for webcam tables.

  A Cloudflare TURN key (`CLOUDFLARE_TURN_KEY_ID` + `CLOUDFLARE_TURN_API_TOKEN`) is a long-term
  secret that stays on the server. Each webcam-table config request exchanges it for ICE servers
  whose username/credential expire after `:ttl` seconds, so nothing durable reaches the browser.
  """

  require Logger

  @endpoint "https://rtc.live.cloudflare.com/v1/turn/keys"
  @default_ttl 6 * 60 * 60

  @doc "True when a TURN key ID and API token are configured."
  def configured? do
    config = config()
    present?(config[:key_id]) and present?(config[:api_token])
  end

  @doc """
  Requests ICE servers carrying fresh TURN credentials.

  Returns `{:ok, servers}` where each server is a map with string keys `"urls"` and, for the
  TURN entry, `"username"` and `"credential"`, or `{:error, reason}`.
  """
  def ice_servers do
    config = config()
    url = "#{@endpoint}/#{config[:key_id]}/credentials/generate-ice-servers"

    options =
      [
        auth: {:bearer, config[:api_token]},
        json: %{ttl: Keyword.get(config, :ttl, @default_ttl)},
        connect_options: [timeout: 3_000],
        receive_timeout: 5_000,
        retry: false
      ]
      |> Keyword.merge(Application.get_env(:the_gathering, :cloudflare_turn_req_options, []))

    case Req.post(url, options) do
      {:ok, %Req.Response{status: status, body: %{"iceServers" => servers}}}
      when status in 200..299 and is_list(servers) ->
        {:ok, Enum.map(servers, &Map.take(&1, ["urls", "username", "credential"]))}

      {:ok, %Req.Response{status: status, body: body}} ->
        Logger.warning(
          "Cloudflare TURN credential request failed with #{status}: #{inspect(body)}"
        )

        {:error, {:status, status}}

      {:error, reason} ->
        Logger.warning("Cloudflare TURN credential request failed: #{inspect(reason)}")
        {:error, reason}
    end
  end

  defp config, do: Application.get_env(:the_gathering, __MODULE__, [])

  defp present?(value), do: is_binary(value) and value != ""
end
