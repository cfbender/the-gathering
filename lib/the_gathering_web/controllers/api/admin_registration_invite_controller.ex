defmodule TheGatheringWeb.API.AdminRegistrationInviteController do
  use TheGatheringWeb, :controller

  alias TheGathering.Accounts

  action_fallback TheGatheringWeb.API.FallbackController

  def show(conn, _params) do
    conn
    |> put_resp_header("cache-control", "no-store")
    |> render(:show, enabled: not is_nil(Accounts.get_settings().registration_invite_hash))
  end

  def create(conn, _params) do
    with {:ok, token} <- Accounts.rotate_registration_invite() do
      conn
      |> put_resp_header("cache-control", "no-store")
      |> render(:create, token: token)
    end
  end
end
