defmodule TheGatheringWeb.API.RegistrationInviteControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  import Ecto.Query
  import ExUnit.CaptureLog
  import TheGathering.AccountsFixtures

  alias TheGathering.Accounts
  alias TheGathering.Accounts.UserToken
  alias TheGathering.Repo

  test "creation and rotation require an administrator with recent authentication" do
    admin = admin_fixture()
    member = user_fixture()
    stale = log_in_user(build_conn(), admin)
    token = get_session(stale, :user_token)

    Repo.update_all(from(t in UserToken, where: t.token == ^token),
      set: [
        authenticated_at:
          DateTime.utc_now() |> DateTime.add(-11, :minute) |> DateTime.truncate(:second)
      ]
    )

    for method <- [:get, :post] do
      assert build_conn() |> request(method) |> json_response(401)
      assert build_conn() |> log_in_user(member) |> request(method) |> json_response(403)

      assert %{"errors" => %{"code" => "sudo_required"}} =
               stale |> request(method) |> json_response(403)
    end

    assert is_nil(Accounts.get_settings().registration_invite_hash)
    conn = build_conn() |> log_in_user(admin) |> post("/api/admin/registration-invite")
    assert %{"data" => %{"token" => first}} = json_response(conn, 200)
    assert get_resp_header(conn, "cache-control") == ["no-store"]
    assert byte_size(first) == 43
    assert Accounts.get_settings().registration_invite_hash == :crypto.hash(:sha256, first)

    conn = conn |> recycle() |> post("/api/admin/registration-invite")
    assert %{"data" => %{"token" => second}} = json_response(conn, 200)
    refute first == second
    refute Accounts.valid_registration_invite_hash?(Accounts.registration_invite_hash(first))
    assert Accounts.valid_registration_invite_hash?(Accounts.registration_invite_hash(second))
    conn = conn |> recycle() |> get("/api/admin/registration-invite")
    assert json_response(conn, 200) == %{"data" => %{"enabled" => true}}
  end

  test "public settings and logs do not reveal secrets; invalid tokens clear pending invitations" do
    admin_fixture()
    {:ok, token} = Accounts.rotate_registration_invite()
    hash = Accounts.registration_invite_hash(token)
    previous_level = Logger.level()
    on_exit(fn -> Logger.configure(level: previous_level) end)

    assert %{"data" => %{"allowed" => false, "bootstrap" => false, "discord_configured" => true}} =
             build_conn() |> get("/api/registration") |> json_response(200)

    log =
      capture_log([level: :debug], fn ->
        Logger.configure(level: :debug)
        conn = post(build_conn(), "/api/registration-invite", %{token: token})
        assert json_response(conn, 200) == %{"data" => %{"valid" => true}}
        assert get_session(conn, :registration_invite_hash) == hash

        assert conn |> recycle() |> get("/api/registration-invite") |> json_response(200) == %{
                 "data" => %{"valid" => true}
               }

        for invalid <- [nil, "", "invalid", String.duplicate("x", 43), hash, %{"token" => token}] do
          rejected = conn |> recycle() |> post("/api/registration-invite", %{token: invalid})
          assert json_response(rejected, 200) == %{"data" => %{"valid" => false}}
          refute get_session(rejected, :registration_invite_hash)
        end

        {:ok, _new_token} = Accounts.rotate_registration_invite()

        assert conn |> recycle() |> get("/api/registration-invite") |> json_response(200) == %{
                 "data" => %{"valid" => false}
               }

        assert build_conn()
               |> post("/api/registration-invite", %{token: token})
               |> json_response(200) == %{"data" => %{"valid" => false}}
      end)

    Logger.configure(level: previous_level)
    assert log =~ "[FILTERED]"
    refute log =~ token
    refute log =~ inspect(hash)

    refute inspect(Accounts.get_settings()) =~
             inspect(Accounts.get_settings().registration_invite_hash)
  end

  test "invite fields cannot be assigned through settings or password registration", %{conn: conn} do
    admin = admin_fixture()
    {:ok, token} = Accounts.rotate_registration_invite()
    hash = Accounts.get_settings().registration_invite_hash

    conn =
      conn
      |> log_in_user(admin)
      |> patch("/api/admin/settings", %{settings: %{registration_invite_hash: "attacker"}})

    assert json_response(conn, 200) == %{
             "data" => %{"registration_enabled" => false, "detailed_stats_from" => nil}
           }

    assert Accounts.get_settings().registration_invite_hash == hash
    conn = build_conn() |> post("/api/registration-invite", %{token: token}) |> recycle()
    assert conn |> post("/api/users", %{user: valid_user_attributes()}) |> json_response(403)
  end

  defp request(conn, :get), do: get(conn, "/api/admin/registration-invite")
  defp request(conn, :post), do: post(conn, "/api/admin/registration-invite")
end
