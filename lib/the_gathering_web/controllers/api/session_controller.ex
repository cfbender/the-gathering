defmodule TheGatheringWeb.API.SessionController do
  use TheGatheringWeb, :controller

  alias TheGathering.Accounts
  alias TheGatheringWeb.API.UserJSON
  alias TheGatheringWeb.UserAuth

  action_fallback TheGatheringWeb.API.FallbackController

  def show(%{assigns: %{current_scope: %{user: nil}}}, _params), do: {:error, :unauthorized}

  def show(conn, _params) do
    conn |> put_view(UserJSON) |> render(:show, user: conn.assigns.current_scope.user)
  end

  def create(conn, %{"username" => username, "password" => password}) do
    case Accounts.get_user_by_username_and_password(username, password) do
      %Accounts.User{} = user ->
        conn
        |> UserAuth.log_in_user(user)
        |> put_view(UserJSON)
        |> render(:show, user: user)

      nil ->
        {:error, :unauthorized}
    end
  end

  def create(_conn, _params), do: {:error, :unauthorized}

  def delete(conn, _params) do
    conn
    |> UserAuth.log_out_user()
    |> send_resp(:no_content, "")
  end

  def sudo(conn, %{"password" => password}) do
    user = conn.assigns.current_scope.user

    case Accounts.get_user_by_username_and_password(user.username, password) do
      %Accounts.User{} = reauthenticated_user ->
        conn
        |> UserAuth.log_in_user(reauthenticated_user)
        |> put_view(UserJSON)
        |> render(:show, user: reauthenticated_user)

      nil ->
        {:error, :unauthorized}
    end
  end

  def sudo(_conn, _params), do: {:error, :unauthorized}

  def update_profile(conn, %{"user" => attrs}) do
    with {:ok, user} <- Accounts.update_profile(conn.assigns.current_scope.user, attrs) do
      conn |> put_view(UserJSON) |> render(:show, user: user)
    end
  end

  def update_profile(_conn, _params), do: {:error, :bad_request}

  def update_password(conn, %{"password" => _password} = params) do
    with {:ok, {user, expired_tokens}} <-
           Accounts.update_user_password(
             conn.assigns.current_scope.user,
             Map.take(params, ["password", "password_confirmation"])
           ) do
      UserAuth.disconnect_sessions(expired_tokens)

      conn
      |> UserAuth.log_in_user(user)
      |> put_view(UserJSON)
      |> render(:show, user: user)
    end
  end

  def update_password(_conn, _params), do: {:error, :bad_request}
end
