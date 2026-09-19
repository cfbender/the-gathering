defmodule TheGatheringWeb.API.FallbackController do
  @moduledoc """
  Translates non-`%Plug.Conn{}` controller results into JSON error responses.

  Every `/api` controller should declare `action_fallback TheGatheringWeb.API.FallbackController`
  and return one of the tuples below from actions instead of rendering errors by hand:

    * `{:error, %Ecto.Changeset{}}` -> 422 with `{"errors": {"field": ["message"]}}`
    * `{:error, :not_found}`        -> 404
    * `{:error, :unauthorized}`     -> 401 (not signed in)
    * `{:error, :forbidden}`        -> 403 (signed in, not allowed)
    * `{:error, :bad_request}`      -> 400

  Also serves the JSON 404 for API paths that match no route.
  """
  use TheGatheringWeb, :controller

  alias TheGatheringWeb.ChangesetJSON

  # Routed requests (the JSON 404 below) still dispatch through Phoenix.
  def call(conn, action) when is_atom(action), do: super(conn, action)

  def call(conn, {:error, %Ecto.Changeset{} = changeset}) do
    conn
    |> put_status(:unprocessable_entity)
    |> json(ChangesetJSON.error(%{changeset: changeset}))
  end

  def call(conn, {:error, status})
      when status in [:bad_request, :unauthorized, :forbidden, :not_found] do
    conn
    |> put_status(status)
    |> json(%{errors: %{detail: Plug.Conn.Status.reason_phrase(status_code(status))}})
  end

  @doc "JSON 404 for API paths that match no route."
  def not_found(conn, _params), do: call(conn, {:error, :not_found})

  defp status_code(status), do: Plug.Conn.Status.code(status)
end
