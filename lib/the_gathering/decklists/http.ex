defmodule TheGathering.Decklists.HTTP do
  @moduledoc false

  alias TheGathering.Decklists.{Budget, Destination}

  @user_agent "TheGathering/0.1 deck metadata resolver (+https://github.com/cfbender/the-gathering)"

  def get(url, headers \\ []) do
    request([method: :get, url: url], headers)
  end

  def post(url, json) do
    request([method: :post, url: url, json: json], [])
  end

  def get_limited(url, budget, headers \\ []) do
    limited_request([method: :get, url: url], headers, budget)
  end

  def get_remote(origin, path, headers, budget) do
    with {:ok, uri, address} <- resolve_within_budget(origin, budget) do
      request_uri = URI.merge(uri, path)
      pinned_uri = %{request_uri | host: address |> :inet.ntoa() |> to_string()}
      host = authority(uri)

      options = [
        method: :get,
        url: URI.to_string(pinned_uri),
        redirect: false,
        connect_options: connection_options(uri.host, address)
      ]

      limited_request(options, [{"host", host} | headers], budget)
    end
  end

  defp resolve_within_budget(origin, budget) do
    case Budget.remaining_ms(budget) do
      0 ->
        {:error, :duration_limit}

      remaining ->
        task = Task.async(fn -> Destination.resolve(origin) end)

        case Task.yield(task, remaining) || Task.shutdown(task) do
          {:ok, result} -> result
          nil -> {:error, :duration_limit}
        end
    end
  end

  defp request(options, headers) do
    defaults = [
      headers: [{"accept", "application/json"}, {"user-agent", @user_agent}] ++ headers,
      connect_options: [timeout: 3_000],
      receive_timeout: 5_000,
      retry: false
    ]

    req_options = Application.get_env(:the_gathering, :decklists_req_options, [])
    Req.request(Keyword.merge(defaults, req_options) ++ options)
  end

  defp limited_request(options, headers, budget) do
    remaining = Budget.remaining_ms(budget)

    if remaining == 0 do
      {:error, :duration_limit}
    else
      into = &stream_body(&1, &2, budget)

      options =
        options
        |> put_connect_timeout(remaining)
        |> Kernel.++(
          into: into,
          decode_body: false,
          compressed: false,
          receive_timeout: min(remaining, 5_000)
        )

      case request(options, headers) do
        {:ok, %Req.Response{body: :body_too_large}} ->
          {:error, :byte_limit}

        {:ok, %Req.Response{} = response} ->
          body = response.body |> IO.iodata_to_binary() |> decode_json()
          {:ok, %{response | body: body}}

        other ->
          other
      end
    end
  end

  defp stream_body({:data, data}, {request, response}, budget) do
    case Budget.consume(budget, byte_size(data)) do
      :ok ->
        response = %{response | body: [response.body, data]}
        {:cont, {request, response}}

      {:error, :byte_limit} ->
        {:halt, {request, %{response | body: :body_too_large}}}
    end
  end

  defp decode_json(body) do
    case Jason.decode(body) do
      {:ok, decoded} -> decoded
      {:error, _reason} -> body
    end
  end

  defp put_connect_timeout(options, remaining) do
    Keyword.update(
      options,
      :connect_options,
      [timeout: min(remaining, 3_000)],
      fn connect_options ->
        Keyword.put(connect_options, :timeout, min(remaining, 3_000))
      end
    )
  end

  defp connection_options(host, address) do
    options = [hostname: host, timeout: 3_000]

    if tuple_size(address) == 8 do
      Keyword.put(options, :transport_opts, inet6: true)
    else
      options
    end
  end

  defp authority(%URI{host: host, port: port, scheme: scheme}) do
    host = if String.contains?(host, ":"), do: "[#{host}]", else: host
    default_port = if scheme == "https", do: 443, else: 80
    if port == default_port, do: host, else: "#{host}:#{port}"
  end
end
