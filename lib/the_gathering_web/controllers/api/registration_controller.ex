defmodule TheGatheringWeb.API.RegistrationController do
  use TheGatheringWeb, :controller

  alias TheGathering.Accounts
  alias TheGatheringWeb.API.UserJSON
  alias TheGatheringWeb.UserAuth

  action_fallback TheGatheringWeb.API.FallbackController

  def show(conn, _params), do: json(conn, %{data: Accounts.registration_status()})

  def create(conn, %{"user" => attrs}) do
    case Accounts.register_user(attrs) do
      {:ok, user} ->
        conn
        |> UserAuth.log_in_user(user)
        |> put_status(:created)
        |> put_view(UserJSON)
        |> render(:show, user: user)

      {:error, :registration_closed} ->
        {:error, :forbidden}

      {:error, changeset} ->
        {:error, changeset}
    end
  end

  def create(_conn, _params), do: {:error, :bad_request}
end
