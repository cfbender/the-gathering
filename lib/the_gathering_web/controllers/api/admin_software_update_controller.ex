defmodule TheGatheringWeb.API.AdminSoftwareUpdateController do
  use TheGatheringWeb, :controller

  alias TheGathering.SelfUpdate

  action_fallback TheGatheringWeb.API.FallbackController

  @doc "Running version, update channel, configured updater, and the newest build on GitHub."
  def show(conn, _params) do
    conn
    |> put_resp_header("cache-control", "no-store")
    |> render(:show, status: SelfUpdate.status())
  end

  @doc """
  Asks the configured updater to install the newest build of this server's channel. The server
  restarts once that updater has finished, so the 202 only says the request was handed over.
  """
  def create(conn, _params) do
    case SelfUpdate.request_update() do
      {:ok, status} ->
        conn
        |> put_status(:accepted)
        |> put_resp_header("cache-control", "no-store")
        |> render(:show, status: status)

      {:error, :unsupported} ->
        {:error, :bad_request}

      {:error, :update_in_progress} ->
        {:error, :conflict}

      {:error, :updater_unavailable} ->
        {:error, :bad_gateway}
    end
  end
end
