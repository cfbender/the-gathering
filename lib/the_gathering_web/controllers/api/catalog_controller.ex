defmodule TheGatheringWeb.API.CatalogController do
  use TheGatheringWeb, :controller

  alias TheGathering.Catalog

  action_fallback TheGatheringWeb.API.FallbackController

  def show(conn, _params), do: render(conn, :show, sync: Catalog.sync_status())

  def sync(conn, _params) do
    result = Catalog.trigger_sync()

    conn
    |> put_status(:accepted)
    |> render(:triggered, result: result)
  end
end
