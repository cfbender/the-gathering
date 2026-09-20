defmodule TheGatheringWeb.API.RemoteDeckController do
  use TheGatheringWeb, :controller

  alias TheGathering.Decklists.RemoteDecks
  alias TheGathering.Games
  alias TheGatheringWeb.API.RemoteDeckJSON

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, _params) do
    result = RemoteDecks.list(conn.assigns.current_scope.user)
    conn |> put_view(RemoteDeckJSON) |> render(:index, result: result)
  end

  def sync(conn, _params) do
    with {:ok, result} <- Games.sync_remote_decks(conn.assigns.current_scope.user) do
      conn |> put_view(RemoteDeckJSON) |> render(:sync, result: result)
    end
  end
end
