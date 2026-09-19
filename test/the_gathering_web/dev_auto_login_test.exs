defmodule TheGatheringWeb.DevAutoLoginTest do
  use TheGatheringWeb.ConnCase

  import TheGathering.AccountsFixtures

  alias TheGathering.Accounts

  setup do
    Application.put_env(:the_gathering, :dev_auto_login, true)
    on_exit(fn -> Application.delete_env(:the_gathering, :dev_auto_login) end)
    :ok
  end

  test "anonymous requests are signed in as a newly created dev admin", %{conn: conn} do
    conn = get(conn, ~p"/api/session")

    assert %{"data" => %{"username" => "dev", "role" => "admin", "has_password" => false}} =
             json_response(conn, 200)

    assert is_binary(get_session(conn, :user_token))
    assert [%{username: "dev"}] = Accounts.list_users()
  end

  test "an existing enabled admin is reused instead of creating dev", %{conn: conn} do
    user_fixture(%{"username" => "member"})
    retired = admin_fixture(%{"username" => "retired"})
    admin_fixture(%{"username" => "owner"})
    {:ok, _} = Accounts.disable_user(retired)

    conn = get(conn, ~p"/api/session")

    assert %{"data" => %{"username" => "owner"}} = json_response(conn, 200)
    refute Accounts.get_user_by_username("dev")
  end

  test "admin routes skip sudo re-authentication", %{conn: conn} do
    conn = get(conn, ~p"/api/admin/settings")

    assert %{"data" => %{"registration_enabled" => _}} = json_response(conn, 200)
  end

  test "auto login is off unless configured", %{conn: conn} do
    Application.delete_env(:the_gathering, :dev_auto_login)

    assert json_response(get(conn, ~p"/api/session"), 401)
  end
end
