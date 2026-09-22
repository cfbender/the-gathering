defmodule TheGatheringWeb.API.WebcamTableConfigControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  setup :register_and_log_in_user

  test "returns authenticated ICE configuration", %{conn: conn} do
    previous = Application.get_env(:the_gathering, :webcam_table)

    Application.put_env(:the_gathering, :webcam_table,
      stun_urls: ["stun:stun.example:3478"],
      turn_urls: ["turns:turn.example:5349"],
      turn_username: "table",
      turn_credential: "secret"
    )

    on_exit(fn -> Application.put_env(:the_gathering, :webcam_table, previous) end)

    assert %{
             "data" => %{
               "max_players" => 4,
               "minimum_height" => 1080,
               "socket_token" => socket_token,
               "ice_servers" => [
                 %{"urls" => ["stun:stun.example:3478"]},
                 %{
                   "urls" => ["turns:turn.example:5349"],
                   "username" => "table",
                   "credential" => "secret"
                 }
               ]
             }
           } = conn |> get(~p"/api/webcam-table/config") |> json_response(200)

    assert is_binary(socket_token)
  end

  test "requires authentication", %{conn: conn} do
    conn = conn |> recycle() |> init_test_session(%{})
    previous = Application.get_env(:the_gathering, :dev_auto_login, false)
    Application.put_env(:the_gathering, :dev_auto_login, false)
    on_exit(fn -> Application.put_env(:the_gathering, :dev_auto_login, previous) end)

    assert conn |> get(~p"/api/webcam-table/config") |> json_response(401)
  end
end
