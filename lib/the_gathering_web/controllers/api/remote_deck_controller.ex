defmodule TheGatheringWeb.API.RemoteDeckController do
  use TheGatheringWeb, :controller

  alias TheGathering.Decklists.RemoteDecks
  alias TheGatheringWeb.API.RemoteDeckJSON

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, _params) do
    result = RemoteDecks.list(conn.assigns.current_scope.user)
    conn |> put_view(RemoteDeckJSON) |> render(:index, result: result)
  end
end
