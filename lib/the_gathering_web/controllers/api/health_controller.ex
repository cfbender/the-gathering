defmodule TheGatheringWeb.API.HealthController do
  use TheGatheringWeb, :controller

  alias Ecto.Adapters.SQL
  alias TheGathering.Repo

  @doc "Liveness/readiness probe used by the container healthcheck."
  def show(conn, _params) do
    case SQL.query(Repo, "SELECT 1", []) do
      {:ok, _result} ->
        json(conn, %{status: "ok"})

      {:error, _reason} ->
        conn
        |> put_status(:service_unavailable)
        |> json(%{status: "error", database: "unavailable"})
    end
  end
end
