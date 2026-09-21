defmodule TheGatheringWeb.API.PortableImportController do
  use TheGatheringWeb, :controller
  alias TheGathering.Imports

  action_fallback TheGatheringWeb.API.FallbackController

  def export(conn, _params) do
    with {:ok, data} <- Imports.export_portable() do
      conn
      |> put_resp_header("cache-control", "no-store")
      |> send_download({:binary, Jason.encode!(data, pretty: true)},
        filename: "the-gathering-#{Date.utc_today()}.json",
        content_type: "application/json"
      )
    end
  end

  def preview(conn, %{"json" => json}) when is_binary(json),
    do: respond(conn, Imports.preview_portable(json))

  def preview(_conn, _params), do: {:error, :bad_request}

  def create(conn, %{"json" => json}) when is_binary(json),
    do: respond(conn, Imports.import_portable(json, conn.assigns.current_scope.user.id))

  def create(_conn, _params), do: {:error, :bad_request}

  defp respond(conn, {:ok, result}), do: render(conn, :show, data: result)

  defp respond(_conn, {:error, message}) when is_binary(message) do
    {:error,
     Ecto.Changeset.add_error(Ecto.Changeset.change({%{}, %{import: :string}}), :import, message)}
  end
end
