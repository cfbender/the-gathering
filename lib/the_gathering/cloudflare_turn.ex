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

  # Cloudflare returns six TURN URLs: primary and alternate ports for UDP, TCP, and TLS. A
  # browser opens one relay allocation per URL on every peer connection, and Firefox warns that
  # five or more STUN/TURN URLs slow discovery, so only two are passed on: UDP on 3478, and TLS
  # on 443 for networks that block UDP. The alternate ports (53, 80, 5349) and plain TCP add no
  # reachability those two lack, and browsers refuse port 53 outright.
  @preferred_turn_urls [
    ~r/^turn:[^?]*:3478\?transport=udp$/,
    ~r/^turns:[^?]*:443\?transport=tcp$/
  ]

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
        {:ok,
         servers
         |> Enum.map(&Map.take(&1, ["urls", "username", "credential"]))
         |> Enum.map(&prefer_urls/1)}

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

  # Keeps the preferred TURN URLs when the response has them; an unfamiliar URL set (or a
  # STUN-only entry) passes through untouched rather than losing the relay.
  defp prefer_urls(%{"urls" => urls} = server) when is_list(urls) do
    case Enum.filter(urls, fn url -> Enum.any?(@preferred_turn_urls, &Regex.match?(&1, url)) end) do
      [] -> server
      preferred -> %{server | "urls" => preferred}
    end
  end

  defp prefer_urls(server), do: server

  defp config, do: Application.get_env(:the_gathering, __MODULE__, [])

  defp present?(value), do: is_binary(value) and value != ""
end
