defmodule TheGatheringWeb.ApiFallbackController do
  use TheGatheringWeb, :controller

  @doc "JSON 404 for API paths that match no route."
  def not_found(conn, _params) do
    conn
    |> put_status(:not_found)
    |> json(%{errors: %{detail: "Not Found"}})
  end
end
