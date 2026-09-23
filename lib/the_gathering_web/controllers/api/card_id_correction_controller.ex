defmodule TheGatheringWeb.API.CardIdCorrectionController do
  use TheGatheringWeb, :controller

  alias TheGathering.CardId.Corrections

  action_fallback TheGatheringWeb.API.FallbackController
  plug TheGatheringWeb.RateLimit, [bucket: :corrections] when action in [:create]
  plug TheGatheringWeb.CardIdExportAuth when action in [:index, :crop]

  def create(conn, params) do
    with {:ok, correction} <- Corrections.save(params, conn.assigns.current_scope.user.id) do
      conn |> put_status(:created) |> render(:show, correction: correction)
    end
  end

  def index(conn, params) do
    with value when is_binary(value) <- Map.get(params, "cursor", "0"),
         {cursor, ""} when cursor >= 0 <- Integer.parse(value) do
      render(conn, :index, page: Corrections.page(cursor))
    else
      _ -> {:error, :bad_request}
    end
  end

  def crop(conn, %{"id" => id}) do
    with {:ok, path} <- Corrections.crop_path(id) do
      conn
      |> put_resp_header("cache-control", "private, no-store")
      |> put_resp_content_type("image/jpeg")
      |> send_file(200, path)
    end
  end
end
