defmodule TheGatheringWeb.DiscordAuthControllerTest do
  use TheGatheringWeb.ConnCase

  import Ecto.Query
  import ExUnit.CaptureLog

  alias TheGathering.Accounts
  alias TheGathering.Accounts.UserToken
  alias TheGathering.Games.Player
  alias TheGathering.Repo

  setup do
    Req.Test.stub(TheGathering.DiscordOAuth, &discord_response/1)
    :ok
  end

  test "callback creates a passwordless member and linked player when registration is open", %{
    conn: conn
  } do
    create_admin()
    open_registration()

    conn = discord_callback(conn, "100000000000000001", "/games")

    assert redirected_to(conn) == "/games"

    assert {user, _inserted_at} =
             conn |> get_session(:user_token) |> Accounts.get_user_by_session_token()

    assert user.discord_id == "100000000000000001"
    assert user.username == "discord_user"
    assert user.role == "member"
    assert is_nil(user.hashed_password)
    assert user.avatar_url == "https://cdn.discordapp.com/avatars/100000000000000001/avatar-hash"

    assert %Player{user_id: user_id, name: "Discord_User"} =
             Repo.get_by(Player, discord_id: user.discord_id)

    assert user_id == user.id
  end

  test "callback rejects an unknown Discord account when registration is closed", %{conn: conn} do
    create_admin()

    conn = discord_callback(conn, "100000000000000002")

    assert redirected_to(conn) == "/login?error=registration_closed"
    refute Accounts.get_user_by_discord_id("100000000000000002")
    refute get_session(conn, :user_token)
  end

  test "one invitation admits multiple passwordless members without opening registration" do
    create_admin()
    {:ok, token} = Accounts.rotate_registration_invite()

    for id <- ["200000000000000001", "200000000000000002"] do
      conn = build_conn() |> accept_invite(token) |> discord_callback(id)
      assert redirected_to(conn) == "/"
      assert {user, _} = Accounts.get_user_by_session_token(get_session(conn, :user_token))
      assert user.role == "member"
      assert is_nil(user.hashed_password)
      assert %Player{user_id: user_id} = Repo.get_by(Player, discord_id: id)
      assert user_id == user.id
      refute get_session(conn, :discord_oauth)
      refute get_session(conn, :registration_invite_hash)
    end

    assert %{allowed: false, bootstrap: false} = Accounts.registration_status()
  end

  test "an invitation survives restarting OAuth before completing registration", %{conn: conn} do
    create_admin()
    {:ok, token} = Accounts.rotate_registration_invite()
    conn = conn |> accept_invite(token) |> get("/auth/discord")

    conn = conn |> recycle() |> discord_callback("200000000000000010")

    assert redirected_to(conn) == "/"
    assert Accounts.get_user_by_discord_id("200000000000000010")
    refute get_session(conn, :registration_invite_hash)
    refute get_session(conn, :discord_oauth)
  end

  @tag capture_log: true
  test "an invitation survives a canceled OAuth attempt and can be retried", %{conn: conn} do
    create_admin()
    {:ok, token} = Accounts.rotate_registration_invite()
    conn = conn |> accept_invite(token) |> get("/auth/discord")

    conn =
      conn
      |> recycle()
      |> get("/auth/discord/callback", %{error: "access_denied", state: oauth_state(conn)})

    assert redirected_to(conn) == "/login?error=discord_failed"
    refute get_session(conn, :discord_oauth)
    refute get_session(conn, :user_token)

    conn = conn |> recycle() |> discord_callback("200000000000000011")

    assert redirected_to(conn) == "/"
    assert Accounts.get_user_by_discord_id("200000000000000011")
    refute get_session(conn, :registration_invite_hash)
  end

  test "invalid invitations cannot register a new Discord member", %{conn: conn} do
    create_admin()
    {:ok, _token} = Accounts.rotate_registration_invite()
    conn = post(conn, "/api/registration-invite", %{token: String.duplicate("x", 43)})
    assert json_response(conn, 200) == %{"data" => %{"valid" => false}}
    conn = conn |> recycle() |> discord_callback("200000000000000009")
    assert redirected_to(conn) == "/login?error=registration_closed"
    refute Accounts.get_user_by_discord_id("200000000000000009")
    refute get_session(conn, :user_token)
  end

  test "rotation revokes an invitation after OAuth starts, not just at landing", %{conn: conn} do
    create_admin()
    {:ok, token} = Accounts.rotate_registration_invite()
    conn = conn |> accept_invite(token) |> get("/auth/discord")
    state = oauth_state(conn)
    refute redirected_to(conn) =~ token

    assert get_session(conn, :registration_invite_hash) ==
             Accounts.registration_invite_hash(token)

    assert get_session(conn, :discord_oauth).registration_invite_hash

    {:ok, new_token} = Accounts.rotate_registration_invite()
    conn = finish_callback(conn, "200000000000000003", state)
    assert redirected_to(conn) == "/login?error=registration_closed"
    refute Accounts.get_user_by_discord_id("200000000000000003")
    refute get_session(conn, :discord_oauth)
    refute get_session(conn, :user_token)

    conn = conn |> recycle() |> discord_callback("200000000000000003")
    assert redirected_to(conn) == "/login?error=registration_closed"
    refute Accounts.get_user_by_discord_id("200000000000000003")
    refute get_session(conn, :user_token)

    conn = build_conn() |> accept_invite(new_token) |> discord_callback("200000000000000003")
    assert redirected_to(conn) == "/"
  end

  test "rotation before OAuth starts revokes the accepted invitation", %{conn: conn} do
    create_admin()
    {:ok, token} = Accounts.rotate_registration_invite()
    conn = accept_invite(conn, token)
    {:ok, _new_token} = Accounts.rotate_registration_invite()
    conn = discord_callback(conn, "200000000000000004")
    assert redirected_to(conn) == "/login?error=registration_closed"
    refute Accounts.get_user_by_discord_id("200000000000000004")
  end

  test "an invitation cannot bypass OAuth state verification or be supplied at callback", %{
    conn: conn
  } do
    create_admin()
    {:ok, token} = Accounts.rotate_registration_invite()
    conn = conn |> accept_invite(token) |> get("/auth/discord")
    conn = finish_callback(conn, "200000000000000005", "wrong-state")
    assert redirected_to(conn) == "/login?error=discord_failed"
    refute get_session(conn, :discord_oauth)
    refute Accounts.get_user_by_discord_id("200000000000000005")

    conn = get(build_conn(), "/auth/discord")
    state = oauth_state(conn)

    conn =
      conn
      |> recycle()
      |> get("/auth/discord/callback", %{
        "code" => "discord-code:200000000000000005",
        "state" => state,
        "token" => token
      })

    assert redirected_to(conn) == "/login?error=registration_closed"
    refute Accounts.get_user_by_discord_id("200000000000000005")
  end

  test "valid invitations cannot bypass disabled accounts or password bootstrap", %{conn: conn} do
    {:ok, token} = Accounts.rotate_registration_invite()
    conn = conn |> accept_invite(token) |> discord_callback("200000000000000006")
    assert redirected_to(conn) == "/login?error=registration_closed"
    assert %{bootstrap: true} = Accounts.registration_status()

    create_admin()
    conn = build_conn() |> accept_invite(token) |> discord_callback("200000000000000006")
    assert redirected_to(conn) == "/"
    user = Accounts.get_user_by_discord_id("200000000000000006")
    {:ok, _user} = Accounts.disable_user(user)
    conn = build_conn() |> accept_invite(token) |> discord_callback(user.discord_id)
    assert redirected_to(conn) == "/login?error=account_disabled"
    refute get_session(conn, :user_token)
  end

  test "revoked invitations do not block existing members or normal open registration" do
    create_admin()
    open_registration()
    user = create_discord_user("200000000000000007")
    close_registration()
    {:ok, token} = Accounts.rotate_registration_invite()
    existing = accept_invite(build_conn(), token)
    newcomer = accept_invite(build_conn(), token)
    {:ok, _new_token} = Accounts.rotate_registration_invite()

    assert existing |> discord_callback(user.discord_id) |> redirected_to() == "/"
    open_registration()
    assert newcomer |> discord_callback("200000000000000008") |> redirected_to() == "/"
  end

  test "OAuth failures log only their class and status", %{conn: conn} do
    sentinel = "sentinel-discord-oauth-body"

    Req.Test.stub(TheGathering.DiscordOAuth, fn conn ->
      conn
      |> Map.put(:status, 400)
      |> Req.Test.json(%{"error" => sentinel})
    end)

    log =
      capture_log(fn ->
        conn = discord_callback(conn, "100000000000000099")
        assert redirected_to(conn) == "/login?error=discord_failed"
      end)

    refute log =~ sentinel
    assert log =~ "Assent.InvalidResponseError status=400"
  end

  test "a rejected Discord account is created once the administrator opens registration", %{
    conn: conn
  } do
    admin = create_admin()

    conn = discord_callback(conn, "100000000000000007")
    assert redirected_to(conn) == "/login?error=registration_closed"

    admin_conn =
      build_conn()
      |> log_in_user(admin)
      |> patch(~p"/api/admin/settings", %{settings: %{registration_enabled: true}})

    assert json_response(admin_conn, 200) == %{
             "data" => %{"registration_enabled" => true, "detailed_stats_from" => nil}
           }

    conn = discord_callback(build_conn(), "100000000000000007")

    assert redirected_to(conn) == "/"
    assert %{role: "member"} = Accounts.get_user_by_discord_id("100000000000000007")
  end

  test "username and player-name clashes get a short numeric suffix", %{conn: conn} do
    create_admin()
    open_registration()
    # "discord_user" and "Discord_User" are what the stubbed Discord profile yields.
    {:ok, _first} =
      Accounts.create_user(%{
        "username" => "discord_user",
        "display_name" => "Discord_User",
        "password" => "long-enough-password"
      })

    {:ok, _player} = TheGathering.Games.create_player(%{name: "discord_user"})
    {:ok, _player} = TheGathering.Games.create_player(%{name: "discord_user (2)"})

    conn = discord_callback(conn, "100000000000000008")

    assert redirected_to(conn) == "/"

    assert %{username: "discord_user2"} =
             user = Accounts.get_user_by_discord_id("100000000000000008")

    assert %Player{name: "Discord_User (3)"} = Repo.get_by(Player, discord_id: user.discord_id)

    conn = discord_callback(build_conn(), "100000000000000009")
    assert redirected_to(conn) == "/"
    assert %{username: "discord_user3"} = Accounts.get_user_by_discord_id("100000000000000009")
  end

  test "an administrator can rename a member's username", %{conn: conn} do
    admin = create_admin()
    open_registration()
    user = create_discord_user("100000000000000010")

    conn =
      conn
      |> log_in_user(admin)
      |> patch(~p"/api/admin/users/#{user.id}", %{user: %{username: " Wax.Poetik "}})

    assert %{"data" => %{"username" => "wax.poetik"}} = json_response(conn, 200)

    conn =
      build_conn()
      |> log_in_user(admin)
      |> patch(~p"/api/admin/users/#{user.id}", %{user: %{username: "owner"}})

    assert %{"errors" => %{"username" => ["has already been taken"]}} = json_response(conn, 422)
  end

  test "callback signs in an existing linked member while registration is closed", %{conn: conn} do
    create_admin()
    open_registration()
    user = create_discord_user("100000000000000003")
    close_registration()

    conn = discord_callback(conn, user.discord_id)

    assert redirected_to(conn) == "/"

    assert {signed_in_user, _inserted_at} =
             conn |> get_session(:user_token) |> Accounts.get_user_by_session_token()

    assert signed_in_user.id == user.id
  end

  test "callback rejects a disabled linked member", %{conn: conn} do
    create_admin()
    open_registration()
    user = create_discord_user("100000000000000004")
    {:ok, _disabled_user} = Accounts.disable_user(user)

    conn = discord_callback(conn, user.discord_id)

    assert redirected_to(conn) == "/login?error=account_disabled"
    refute get_session(conn, :user_token)
  end

  test "password login fails for a passwordless Discord member", %{conn: conn} do
    create_admin()
    open_registration()
    user = create_discord_user("100000000000000005")

    conn = post(conn, ~p"/api/session", %{username: user.username, password: "any-long-password"})

    assert json_response(conn, 401) == %{"errors" => %{"detail" => "Unauthorized"}}
  end

  test "sudo OAuth refreshes authentication for the currently linked Discord identity", %{
    conn: conn
  } do
    create_admin()
    open_registration()
    user = create_discord_user("100000000000000006")
    conn = log_in_user(conn, user)
    old_token = get_session(conn, :user_token)

    Repo.update_all(
      from(token in UserToken, where: token.token == ^old_token),
      set: [
        authenticated_at:
          DateTime.utc_now() |> DateTime.add(-11, :minute) |> DateTime.truncate(:second)
      ]
    )

    conn = discord_callback(conn, user.discord_id, "/admin/users", %{"sudo" => "1"})

    assert redirected_to(conn) == "/admin/users"
    new_token = get_session(conn, :user_token)
    assert new_token != old_token
    assert {reauthenticated, _inserted_at} = Accounts.get_user_by_session_token(new_token)
    assert Accounts.sudo_mode?(reauthenticated, -1)
  end

  defp discord_callback(conn, discord_id, return_to \\ "/", request_params \\ %{}) do
    request_params = Map.put(request_params, "returnTo", return_to)
    conn = get(conn, "/auth/discord?" <> URI.encode_query(request_params))
    finish_callback(conn, discord_id, oauth_state(conn))
  end

  defp oauth_state(conn) do
    conn
    |> redirected_to()
    |> URI.parse()
    |> Map.fetch!(:query)
    |> URI.decode_query()
    |> Map.fetch!("state")
  end

  defp finish_callback(conn, discord_id, state) do
    conn
    |> recycle()
    |> get(
      "/auth/discord/callback?" <>
        URI.encode_query(%{code: "discord-code:#{discord_id}", state: state})
    )
  end

  defp accept_invite(conn, token) do
    conn = post(conn, "/api/registration-invite", %{token: token})
    assert json_response(conn, 200) == %{"data" => %{"valid" => true}}
    recycle(conn)
  end

  defp discord_response(%{method: "POST", request_path: "/api/oauth2/token"} = conn) do
    %{"code" => "discord-code:" <> discord_id} = conn.body_params

    Req.Test.json(conn, %{
      "access_token" => discord_id,
      "token_type" => "Bearer",
      "expires_in" => 3600,
      "scope" => "identify email"
    })
  end

  defp discord_response(%{method: "GET", request_path: "/api/users/@me"} = conn) do
    ["Bearer " <> discord_id] = Plug.Conn.get_req_header(conn, "authorization")

    Req.Test.json(conn, %{
      "id" => discord_id,
      "username" => "Discord_User",
      "avatar" => "avatar-hash",
      "email" => "member@example.com",
      "verified" => true
    })
  end

  defp create_discord_user(discord_id) do
    assert {:ok, user} =
             Accounts.sign_in_with_discord(%{
               "sub" => discord_id,
               "preferred_username" => "Discord_User",
               "picture" => "https://cdn.discordapp.com/avatars/#{discord_id}/avatar-hash"
             })

    user
  end

  defp create_admin do
    {:ok, admin} =
      Accounts.create_admin(%{
        "username" => "owner",
        "display_name" => "Owner",
        "password" => "long-enough-password"
      })

    admin
  end

  defp open_registration, do: Accounts.update_settings(%{"registration_enabled" => true})
  defp close_registration, do: Accounts.update_settings(%{"registration_enabled" => false})
end
