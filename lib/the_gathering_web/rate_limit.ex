defmodule TheGatheringWeb.RateLimit do
  @moduledoc """
  Per-client rate limiting for credential endpoints.

      plug TheGatheringWeb.RateLimit, bucket: :credentials

  The `:limit` and `:scale` (window in milliseconds) for each bucket come from
  `config :the_gathering, TheGatheringWeb.RateLimit`, so tests and operators can
  tune them without touching the router. Requests over the limit get a JSON 429
  with a `retry-after` header and are halted.

  Clients are identified by `conn.remote_ip`. Behind a reverse proxy every
  request arrives from the proxy address, so set `TRUST_PROXY_HEADERS=true`
  (`trust_proxy_headers: true` in the same config) to read the client address
  from `x-real-ip` or the last `x-forwarded-for` entry instead. Only enable it
  when a proxy you control sets those headers; otherwise clients can spoof them.
  """

  import Plug.Conn

  alias TheGathering.RateLimiter

  @behaviour Plug

  @impl Plug
  def init(opts), do: Keyword.fetch!(opts, :bucket)

  @impl Plug
  def call(conn, bucket) do
    %{limit: limit, scale: scale} = bucket_config(bucket)

    case RateLimiter.hit({bucket, client_ip(conn)}, scale, limit) do
      {:allow, _count} ->
        conn

      {:deny, retry_after_ms} ->
        conn
        |> put_resp_header("retry-after", Integer.to_string(ceil_seconds(retry_after_ms)))
        |> put_status(:too_many_requests)
        |> Phoenix.Controller.json(%{errors: %{detail: "Too Many Requests"}})
        |> halt()
    end
  end

  @doc "Address used to identify the client, honouring proxy headers when trusted."
  def client_ip(conn) do
    if config()[:trust_proxy_headers],
      do: forwarded_ip(conn) || conn.remote_ip,
      else: conn.remote_ip
  end

  defp forwarded_ip(conn) do
    header =
      case get_req_header(conn, "x-real-ip") do
        [real_ip | _] -> real_ip
        [] -> conn |> get_req_header("x-forwarded-for") |> List.last()
      end

    with true <- is_binary(header),
         candidate = header |> String.split(",") |> List.last() |> String.trim(),
         {:ok, ip} <- :inet.parse_strict_address(String.to_charlist(candidate)) do
      ip
    else
      _ -> nil
    end
  end

  defp bucket_config(bucket) do
    bucket_opts = Keyword.fetch!(config(), bucket)
    %{limit: Keyword.fetch!(bucket_opts, :limit), scale: Keyword.fetch!(bucket_opts, :scale)}
  end

  defp config, do: Application.fetch_env!(:the_gathering, __MODULE__)

  defp ceil_seconds(ms), do: max(div(ms + 999, 1000), 1)
end
