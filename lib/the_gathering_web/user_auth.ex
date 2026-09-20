defmodule TheGatheringWeb.UserAuth do
  @moduledoc "Cookie-session authentication adapted from Phoenix's generated auth module."

  import Plug.Conn
  import Phoenix.Controller

  alias TheGathering.Accounts
  alias TheGathering.Accounts.{Scope, User}
  alias TheGatheringWeb.API.FallbackController

  @session_reissue_age_in_days 7

  def init(action), do: action
  def call(conn, action), do: apply(__MODULE__, action, [conn, []])

  @doc "Creates a tracked session token and logs the user in."
  def log_in_user(conn, user, params \\ %{}) do
    conn
    |> create_or_extend_session(user, params)
    |> delete_session(:user_return_to)
    |> put_fresh_csrf_token()
  end

  @doc "Deletes the tracked session token and clears all cookie-session data."
  def log_out_user(conn) do
    user_token = get_session(conn, :user_token)
    user_token && Accounts.delete_user_session_token(user_token)

    if live_socket_id = get_session(conn, :live_socket_id) do
      TheGatheringWeb.Endpoint.broadcast(live_socket_id, "disconnect", %{})
    end

    conn
    |> renew_session(nil)
    |> put_fresh_csrf_token()
  end

  @doc "Loads the current user scope from a valid database-backed session token."
  def fetch_current_scope_for_user(conn, _opts) do
    with token when not is_nil(token) <- get_session(conn, :user_token),
         {user, token_inserted_at} <- Accounts.get_user_by_session_token(token) do
      conn
      |> assign(:current_scope, Scope.for_user(user))
      |> maybe_reissue_user_session_token(user, token_inserted_at)
    else
      _missing_or_invalid ->
        if dev_auto_login?() do
          auto_log_in_dev_admin(conn)
        else
          assign(conn, :current_scope, Scope.for_user(nil))
        end
    end
  end

  # Development only (`config :the_gathering, dev_auto_login: true`): sign every
  # anonymous request in as an administrator so the app is usable without auth.
  defp dev_auto_login?, do: Application.get_env(:the_gathering, :dev_auto_login, false)

  defp auto_log_in_dev_admin(conn) do
    user = Accounts.get_or_create_dev_admin()
    authenticated_at = DateTime.utc_now() |> DateTime.truncate(:second)

    conn
    |> assign(:current_scope, Scope.for_user(nil))
    |> log_in_user(user)
    |> assign(:current_scope, Scope.for_user(%{user | authenticated_at: authenticated_at}))
  end

  @doc "Requires an authenticated user and otherwise returns the API's 401 JSON response."
  def require_authenticated_user(conn, _opts) do
    if conn.assigns.current_scope.user do
      conn
    else
      conn
      |> maybe_store_return_to()
      |> FallbackController.call({:error, :unauthorized})
      |> halt()
    end
  end

  @doc "Requires a password authentication within the previous ten minutes."
  def require_sudo_mode(conn, _opts) do
    if dev_auto_login?() or Accounts.sudo_mode?(conn.assigns.current_scope.user, -10) do
      conn
    else
      conn |> FallbackController.call({:error, :sudo_required}) |> halt()
    end
  end

  @doc "Requires the current user to have the administrator role."
  def require_admin(
        %{assigns: %{current_scope: %Scope{user: %User{role: "admin"}}}} = conn,
        _opts
      ),
      do: conn

  def require_admin(conn, _opts) do
    conn |> FallbackController.call({:error, :forbidden}) |> halt()
  end

  @doc "Disconnects live sockets associated with expired session tokens."
  def disconnect_sessions(tokens) do
    Enum.each(tokens, fn %{token: token} ->
      TheGatheringWeb.Endpoint.broadcast(user_session_topic(token), "disconnect", %{})
    end)
  end

  defp maybe_reissue_user_session_token(conn, user, token_inserted_at) do
    if DateTime.diff(DateTime.utc_now(), token_inserted_at, :day) >= @session_reissue_age_in_days do
      old_token = get_session(conn, :user_token)
      conn = create_or_extend_session(conn, user, %{})
      Accounts.delete_user_session_token(old_token)
      conn
    else
      conn
    end
  end

  defp create_or_extend_session(conn, user, _params) do
    token = Accounts.generate_user_session_token(user)

    conn
    |> renew_session(user)
    |> put_token_in_session(token)
  end

  defp renew_session(
         %{assigns: %{current_scope: %Scope{user: %User{id: user_id}}}} = conn,
         %User{id: user_id}
       ),
       do: conn

  defp renew_session(conn, _user) do
    Plug.CSRFProtection.delete_csrf_token()

    conn
    |> configure_session(renew: true)
    |> clear_session()
  end

  defp put_token_in_session(conn, token) do
    conn
    |> put_session(:user_token, token)
    |> put_session(:live_socket_id, user_session_topic(token))
  end

  defp put_fresh_csrf_token(conn) do
    put_resp_header(conn, "x-csrf-token", Plug.CSRFProtection.get_csrf_token())
  end

  defp user_session_topic(token), do: "users_sessions:#{Base.url_encode64(token)}"

  defp maybe_store_return_to(%{method: "GET"} = conn) do
    put_session(conn, :user_return_to, current_path(conn))
  end

  defp maybe_store_return_to(conn), do: conn
end
