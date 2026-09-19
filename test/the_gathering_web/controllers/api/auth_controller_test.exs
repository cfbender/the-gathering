defmodule TheGatheringWeb.API.AuthControllerTest do
  use TheGatheringWeb.ConnCase

  import Ecto.Query

  alias TheGathering.Accounts
  alias TheGathering.Accounts.UserToken
  alias TheGathering.Repo

  @password "long-enough-password"

  test "registration signs in the first user without exposing password data", %{conn: conn} do
    conn =
      post(conn, ~p"/api/users", %{
        "user" => %{
          "username" => "Owner",
          "display_name" => "Server Owner",
          "password" => @password
        }
      })

    assert %{"data" => %{"username" => "owner", "role" => "admin"}} = json_response(conn, 201)
    refute conn.resp_body =~ "password"
    token = get_session(conn, :user_token)
    assert is_binary(token)
    refute get_session(conn, :user_id)
    assert get_session(conn, :live_socket_id) == "users_sessions:#{Base.url_encode64(token)}"
    assert {_, _inserted_at} = Accounts.get_user_by_session_token(token)

    conn = get(recycle(conn), ~p"/api/session")
    assert %{"data" => %{"display_name" => "Server Owner"}} = json_response(conn, 200)
  end

  test "registration is forbidden after the first user by default", %{conn: conn} do
    admin = create_user("owner", "admin")

    conn =
      post(conn, ~p"/api/users", %{
        "user" => %{"username" => "member", "password" => @password}
      })

    assert json_response(conn, 403) == %{"errors" => %{"detail" => "Forbidden"}}
    assert admin.role == "admin"
  end

  test "a member gets 403 on admin routes", %{conn: conn} do
    create_user("member", "member")

    conn = conn |> log_in("member") |> recycle() |> get(~p"/api/admin/users")

    assert json_response(conn, 403) == %{"errors" => %{"detail" => "Forbidden"}}
  end

  test "logout clears the session", %{conn: conn} do
    create_user("owner", "admin")

    conn = log_in(conn, "OWNER")
    assert %{"data" => %{"username" => "owner"}} = json_response(conn, 200)
    token = get_session(conn, :user_token)

    conn = delete(recycle(conn), ~p"/api/session")
    assert response(conn, 204)
    refute Accounts.get_user_by_session_token(token)

    conn = get(recycle(conn), ~p"/api/session")
    assert json_response(conn, 401) == %{"errors" => %{"detail" => "Unauthorized"}}
  end

  test "disabled users cannot log in", %{conn: conn} do
    create_user("owner", "admin")
    user = create_user("member", "member")
    {:ok, _user} = Accounts.disable_user(user)

    conn = post(conn, ~p"/api/session", %{username: "member", password: @password})
    assert json_response(conn, 401) == %{"errors" => %{"detail" => "Unauthorized"}}
  end

  test "password change invalidates every old session and issues a new one", %{conn: conn} do
    user = create_user("owner", "admin")
    other_token = Accounts.generate_user_session_token(user)
    conn = log_in(conn, "owner")
    old_token = get_session(conn, :user_token)

    conn =
      patch(recycle(conn), ~p"/api/session/password", %{
        password: "a-brand-new-password",
        password_confirmation: "a-brand-new-password"
      })

    assert %{"data" => %{"username" => "owner"}} = json_response(conn, 200)
    refute Accounts.get_user_by_session_token(old_token)
    refute Accounts.get_user_by_session_token(other_token)

    new_token = get_session(conn, :user_token)
    assert new_token != old_token
    assert {_, _inserted_at} = Accounts.get_user_by_session_token(new_token)
  end

  test "stale authentication requires sudo mode and password reauthentication restores it", %{
    conn: conn
  } do
    create_user("owner", "admin")
    conn = log_in(conn, "owner")
    token = get_session(conn, :user_token)

    Repo.update_all(
      from(user_token in UserToken, where: user_token.token == ^token),
      set: [
        authenticated_at:
          DateTime.utc_now() |> DateTime.add(-11, :minute) |> DateTime.truncate(:second)
      ]
    )

    conn = get(recycle(conn), ~p"/api/admin/users")

    assert json_response(conn, 403) == %{
             "errors" => %{
               "code" => "sudo_required",
               "detail" => "Reauthentication required"
             }
           }

    conn = post(recycle(conn), ~p"/api/session/sudo", %{password: @password})
    assert %{"data" => %{"username" => "owner"}} = json_response(conn, 200)

    conn = get(recycle(conn), ~p"/api/admin/users")
    assert %{"data" => [_user]} = json_response(conn, 200)
  end

  defp log_in(conn, username) do
    post(conn, ~p"/api/session", %{username: username, password: @password})
  end

  defp create_user(username, role) do
    {:ok, user} =
      Accounts.create_user(%{
        "username" => username,
        "display_name" => String.capitalize(username),
        "password" => @password,
        "role" => role
      })

    user
  end
end
