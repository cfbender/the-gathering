defmodule TheGatheringWeb.API.WebcamTableConfigControllerTest do
  use TheGatheringWeb.ConnCase, async: false
  import Phoenix.ChannelTest, only: [socket: 3]

  setup :register_and_log_in_user

  setup %{user: user} do
    # SQLite reuses rolled-back user IDs, so clear any earlier test's count.
    TheGathering.RateLimiter.set({:turn_credentials, user.id}, turn_scale(), 0)
    :ok
  end

  defp turn_scale,
    do:
      Application.fetch_env!(:the_gathering, TheGatheringWeb.RateLimit)[:turn_credentials][:scale]

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
               "max_players" => 10,
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

  test "issues an encrypted socket token that connects as the signed-in user", %{
    conn: conn,
    user: user
  } do
    %{"data" => %{"socket_token" => socket_token}} =
      conn |> get(~p"/api/webcam-table/config") |> json_response(200)

    session_token = get_session(conn, :user_token)
    refute socket_token =~ Base.url_encode64(session_token, padding: false)

    socket = socket(TheGatheringWeb.UserSocket, nil, %{})

    assert {:ok, connected} =
             TheGatheringWeb.UserSocket.connect(%{"token" => socket_token}, socket, %{})

    assert connected.assigns.user.id == user.id
  end

  describe "with a Cloudflare TURN key" do
    setup do
      previous = Application.get_env(:the_gathering, :webcam_table)
      previous_cf = Application.get_env(:the_gathering, TheGathering.CloudflareTurn)
      previous_req = Application.get_env(:the_gathering, :cloudflare_turn_req_options)

      Application.put_env(:the_gathering, :webcam_table,
        stun_urls: ["stun:stun.cloudflare.com:3478"],
        turn_urls: []
      )

      Application.put_env(:the_gathering, TheGathering.CloudflareTurn,
        key_id: "key123",
        api_token: "token-abc",
        ttl: 1234
      )

      Application.put_env(:the_gathering, :cloudflare_turn_req_options,
        plug: {Req.Test, __MODULE__}
      )

      on_exit(fn ->
        Application.put_env(:the_gathering, :webcam_table, previous)
        Application.put_env(:the_gathering, TheGathering.CloudflareTurn, previous_cf)
        Application.put_env(:the_gathering, :cloudflare_turn_req_options, previous_req)
      end)

      :ok
    end

    test "appends minted credentials and drops URLs already served statically", %{conn: conn} do
      Req.Test.expect(__MODULE__, fn req_conn ->
        assert req_conn.method == "POST"
        assert req_conn.request_path == "/v1/turn/keys/key123/credentials/generate-ice-servers"
        assert Plug.Conn.get_req_header(req_conn, "authorization") == ["Bearer token-abc"]
        {:ok, body, req_conn} = Plug.Conn.read_body(req_conn)
        assert Jason.decode!(body) == %{"ttl" => 1234}

        req_conn
        |> Plug.Conn.put_status(201)
        |> Req.Test.json(%{
          "iceServers" => [
            %{"urls" => ["stun:stun.cloudflare.com:3478"]},
            %{
              "urls" => [
                "turn:turn.cloudflare.com:3478?transport=udp",
                "turns:turn.cloudflare.com:443?transport=tcp"
              ],
              "username" => "short-lived-user",
              "credential" => "short-lived-secret"
            }
          ]
        })
      end)

      assert %{
               "data" => %{
                 "ice_servers" => [
                   %{"urls" => ["stun:stun.cloudflare.com:3478"]},
                   %{
                     "urls" => [
                       "turn:turn.cloudflare.com:3478?transport=udp",
                       "turns:turn.cloudflare.com:443?transport=tcp"
                     ],
                     "username" => "short-lived-user",
                     "credential" => "short-lived-secret"
                   }
                 ]
               }
             } = conn |> get(~p"/api/webcam-table/config") |> json_response(200)
    end

    test "falls back to the static servers when Cloudflare rejects the key", %{conn: conn} do
      Req.Test.expect(__MODULE__, fn req_conn ->
        req_conn
        |> Plug.Conn.put_status(401)
        |> Req.Test.json(%{"success" => false, "errors" => [%{"message" => "Unauthorized"}]})
      end)

      assert %{"data" => %{"ice_servers" => [%{"urls" => ["stun:stun.cloudflare.com:3478"]}]}} =
               conn |> get(~p"/api/webcam-table/config") |> json_response(200)
    end
  end

  test "limits credential minting per account", %{conn: conn, user: user} do
    limit =
      Application.fetch_env!(:the_gathering, TheGatheringWeb.RateLimit)[:turn_credentials][:limit]

    TheGathering.RateLimiter.set({:turn_credentials, user.id}, turn_scale(), limit)
    limited = get(conn, ~p"/api/webcam-table/config")

    assert json_response(limited, 429) == %{"errors" => %{"detail" => "Too Many Requests"}}
    assert [_seconds] = get_resp_header(limited, "retry-after")

    other_user = TheGathering.AccountsFixtures.user_fixture()
    TheGathering.RateLimiter.set({:turn_credentials, other_user.id}, turn_scale(), 0)
    other = log_in_user(build_conn(), other_user)
    assert other |> get(~p"/api/webcam-table/config") |> json_response(200)
  end

  test "requires authentication", %{conn: conn} do
    conn = conn |> recycle() |> init_test_session(%{})
    previous = Application.get_env(:the_gathering, :dev_auto_login, false)
    Application.put_env(:the_gathering, :dev_auto_login, false)
    on_exit(fn -> Application.put_env(:the_gathering, :dev_auto_login, previous) end)

    assert conn |> get(~p"/api/webcam-table/config") |> json_response(401)
  end
end
