defmodule TheGatheringWeb.API.WebcamTableConfigController do
  use TheGatheringWeb, :controller

  alias TheGathering.CloudflareTurn

  plug TheGatheringWeb.RateLimit, bucket: :turn_credentials

  @token_salt "webcam table socket"

  def show(conn, _params) do
    config = Application.get_env(:the_gathering, :webcam_table, [])
    socket_token = Phoenix.Token.sign(conn, @token_salt, get_session(conn, :user_token))

    conn
    |> put_resp_header("cache-control", "private, no-store")
    |> json(%{
      data: %{
        ice_servers: ice_servers(config),
        max_players: 10,
        minimum_height: 1080,
        socket_token: socket_token
      }
    })
  end

  # Static servers from the environment first, then Cloudflare's short-lived TURN credentials.
  # A Cloudflare outage degrades to the static list rather than failing the room.
  defp ice_servers(config) do
    static = Enum.reject([stun_server(config), turn_server(config)], &is_nil/1)

    cloudflare =
      if CloudflareTurn.configured?() do
        case CloudflareTurn.ice_servers() do
          {:ok, servers} -> servers
          {:error, _reason} -> []
        end
      else
        []
      end

    static ++ without_known_urls(cloudflare, static)
  end

  defp without_known_urls(servers, known) do
    known_urls = known |> Enum.flat_map(&List.wrap(&1.urls)) |> MapSet.new()

    servers
    |> Enum.map(fn server ->
      urls = server["urls"] |> List.wrap() |> Enum.reject(&MapSet.member?(known_urls, &1))
      %{server | "urls" => urls}
    end)
    |> Enum.reject(&(&1["urls"] == []))
  end

  defp stun_server(config) do
    case Keyword.get(config, :stun_urls, []) do
      [] -> nil
      urls -> %{urls: urls}
    end
  end

  defp turn_server(config) do
    case Keyword.get(config, :turn_urls, []) do
      [] ->
        nil

      urls ->
        %{
          urls: urls,
          username: Keyword.get(config, :turn_username),
          credential: Keyword.get(config, :turn_credential)
        }
    end
  end
end
