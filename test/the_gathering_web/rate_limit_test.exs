defmodule TheGatheringWeb.RateLimitTest do
  # Mutates the application env, so it cannot run alongside other tests.
  use TheGatheringWeb.ConnCase, async: false

  alias TheGatheringWeb.RateLimit

  @config Application.compile_env!(:the_gathering, RateLimit)

  setup do
    on_exit(fn -> Application.put_env(:the_gathering, RateLimit, @config) end)
    :ok
  end

  defp configure(overrides) do
    Application.put_env(:the_gathering, RateLimit, Keyword.merge(@config, overrides))
  end

  defp from(conn, ip), do: %{conn | remote_ip: ip}

  test "credential endpoints share a per-address bucket and answer 429 when exhausted", %{
    conn: conn
  } do
    configure(credentials: [limit: 3, scale: :timer.minutes(5)])
    attacker = {10, 200, 0, 1}
    neighbour = {10, 200, 0, 2}

    for _ <- 1..3 do
      assert conn |> from(attacker) |> post(~p"/api/session", %{}) |> json_response(401)
    end

    denied =
      conn |> from(attacker) |> post(~p"/api/session", %{"username" => "a", "password" => "b"})

    assert json_response(denied, 429) == %{"errors" => %{"detail" => "Too Many Requests"}}
    assert [retry_after] = get_resp_header(denied, "retry-after")
    assert String.to_integer(retry_after) in 1..300

    # Bootstrap registration draws from the same bucket, so an exhausted address
    # cannot switch endpoints to keep guessing.
    assert conn
           |> from(attacker)
           |> post(~p"/api/users", %{"user" => %{"username" => "x", "password" => "y"}})
           |> json_response(429)

    # Other clients are unaffected.
    assert conn |> from(neighbour) |> post(~p"/api/session", %{}) |> json_response(401)
  end

  test "requests under the limit pass through untouched", %{conn: conn} do
    configure(credentials: [limit: 2, scale: :timer.minutes(5)])

    conn = conn |> from({10, 200, 1, 1}) |> get(~p"/api/health")
    assert conn.status == 200
    refute conn.halted
  end

  describe "client_ip/1" do
    test "uses the socket address unless proxy headers are trusted", %{conn: conn} do
      configure(trust_proxy_headers: false)
      conn = conn |> from({192, 168, 0, 9}) |> put_req_header("x-forwarded-for", "203.0.113.5")

      assert RateLimit.client_ip(conn) == {192, 168, 0, 9}
    end

    test "prefers x-real-ip, then the last x-forwarded-for hop, when trusted", %{conn: conn} do
      configure(trust_proxy_headers: true)
      conn = from(conn, {192, 168, 0, 9})

      forwarded = put_req_header(conn, "x-forwarded-for", "1.2.3.4, 203.0.113.5")
      assert RateLimit.client_ip(forwarded) == {203, 0, 113, 5}

      real = put_req_header(forwarded, "x-real-ip", "2001:db8::7")
      assert RateLimit.client_ip(real) == {0x2001, 0xDB8, 0, 0, 0, 0, 0, 7}

      garbage = put_req_header(conn, "x-forwarded-for", "not-an-ip")
      assert RateLimit.client_ip(garbage) == {192, 168, 0, 9}

      assert RateLimit.client_ip(conn) == {192, 168, 0, 9}
    end
  end
end
