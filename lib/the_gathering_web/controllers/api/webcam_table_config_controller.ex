defmodule TheGatheringWeb.API.WebcamTableConfigController do
  use TheGatheringWeb, :controller

  @token_salt "webcam table socket"

  def show(conn, _params) do
    config = Application.get_env(:the_gathering, :webcam_table, [])
    socket_token = Phoenix.Token.sign(conn, @token_salt, get_session(conn, :user_token))

    ice_servers =
      [stun_server(config), turn_server(config)]
      |> Enum.reject(&is_nil/1)

    conn
    |> put_resp_header("cache-control", "private, no-store")
    |> json(%{
      data: %{
        ice_servers: ice_servers,
        max_players: 4,
        minimum_height: 1080,
        socket_token: socket_token
      }
    })
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
