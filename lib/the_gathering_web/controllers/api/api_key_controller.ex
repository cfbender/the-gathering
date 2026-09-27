defmodule TheGatheringWeb.API.ApiKeyController do
  use TheGatheringWeb, :controller

  alias TheGathering.Accounts

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, _params) do
    render(conn, :index, api_keys: Accounts.list_api_keys(conn.assigns.current_scope.user))
  end

  def create(conn, %{"api_key" => attrs}) when is_map(attrs) do
    with {:ok, {token, api_key}} <-
           Accounts.create_api_key(conn.assigns.current_scope.user, Map.take(attrs, ["name"])) do
      conn
      |> put_status(:created)
      |> put_resp_header("cache-control", "private, no-store")
      |> render(:create, api_key: api_key, token: token)
    end
  end

  def create(_conn, _params), do: {:error, :bad_request}

  def delete(conn, %{"id" => id}) do
    with {:ok, _api_key} <- Accounts.delete_api_key(conn.assigns.current_scope.user, id) do
      send_resp(conn, :no_content, "")
    end
  end
end
