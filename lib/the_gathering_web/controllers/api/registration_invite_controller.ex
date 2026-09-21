defmodule TheGatheringWeb.API.RegistrationInviteController do
  use TheGatheringWeb, :controller

  alias TheGathering.Accounts

  action_fallback TheGatheringWeb.API.FallbackController

  def show(conn, _params) do
    valid = Accounts.valid_registration_invite_hash?(get_session(conn, :registration_invite_hash))

    conn
    |> put_resp_header("cache-control", "no-store")
    |> render(:show, valid: valid)
  end

  def create(conn, %{"token" => token}) do
    hash = Accounts.registration_invite_hash(token)

    conn =
      if Accounts.valid_registration_invite_hash?(hash) do
        put_session(conn, :registration_invite_hash, hash)
      else
        delete_session(conn, :registration_invite_hash)
      end

    show(conn, %{})
  end

  def create(_conn, _params), do: {:error, :bad_request}
end
