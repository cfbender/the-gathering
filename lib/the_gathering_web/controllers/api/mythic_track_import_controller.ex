defmodule TheGatheringWeb.API.MythicTrackImportController do
  use TheGatheringWeb, :controller

  alias TheGathering.Imports

  action_fallback TheGatheringWeb.API.FallbackController

  def preview(conn, %{"json" => json}) when is_binary(json) do
    render(conn, :preview, preview: Imports.preview(:mythic_track, json))
  end

  def preview(_conn, _params), do: {:error, :bad_request}

  def create(conn, %{"json" => json}) when is_binary(json) do
    user_id = conn.assigns.current_scope.user.id

    case Imports.import(:mythic_track, json, user_id) do
      {:ok, result} ->
        render(conn, :result, result: result)

      {:error, {:validation, preview}} ->
        conn |> put_status(:unprocessable_entity) |> render(:preview, preview: preview)

      {:error, {:changeset, changeset}} ->
        {:error, changeset}
    end
  end

  def create(_conn, _params), do: {:error, :bad_request}
end
