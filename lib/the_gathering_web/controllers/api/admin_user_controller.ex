defmodule TheGatheringWeb.API.AdminUserController do
  use TheGatheringWeb, :controller

  alias TheGathering.Accounts
  alias TheGatheringWeb.API.UserJSON
  alias TheGatheringWeb.UserAuth

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, _params) do
    conn |> put_view(UserJSON) |> render(:index, users: Accounts.list_users())
  end

  def create(conn, %{"user" => attrs}) do
    with {:ok, user} <- Accounts.create_user(attrs) do
      conn |> put_status(:created) |> put_view(UserJSON) |> render(:show, user: user)
    end
  end

  def create(_conn, _params), do: {:error, :bad_request}

  def update(conn, %{"id" => id, "user" => attrs}) do
    with {:ok, user} <- fetch_user(id),
         {:ok, user} <- Accounts.update_user(user, admin_update_attrs(attrs)) do
      conn |> put_view(UserJSON) |> render(:show, user: user)
    end
  end

  def update(_conn, _params), do: {:error, :bad_request}

  def reset_password(conn, %{"id" => id, "password" => password}) do
    with {:ok, user} <- fetch_user(id),
         {:ok, {user, expired_tokens}} <- Accounts.reset_password(user, %{"password" => password}) do
      UserAuth.disconnect_sessions(expired_tokens)
      conn |> put_view(UserJSON) |> render(:show, user: user)
    end
  end

  def reset_password(_conn, _params), do: {:error, :bad_request}

  def delete(conn, %{"id" => id}) do
    with {:ok, user} <- fetch_user(id),
         {:ok, _user} <- Accounts.disable_user(user) do
      send_resp(conn, :no_content, "")
    end
  end

  defp fetch_user(id) do
    case Accounts.get_user(id) do
      nil -> {:error, :not_found}
      user -> {:ok, user}
    end
  end

  defp admin_update_attrs(attrs) do
    case Map.fetch(attrs, "disabled") do
      {:ok, true} ->
        attrs
        |> Map.delete("disabled")
        |> Map.put("disabled_at", DateTime.utc_now() |> DateTime.truncate(:second))

      {:ok, false} ->
        attrs |> Map.delete("disabled") |> Map.put("disabled_at", nil)

      :error ->
        attrs
    end
  end
end
