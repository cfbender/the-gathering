defmodule TheGatheringWeb.API.WebcamTableRoomController do
  use TheGatheringWeb, :controller

  alias TheGatheringWeb.WebcamTableRooms

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, _params) do
    conn
    |> put_resp_header("cache-control", "private, no-store")
    |> render(:index, rooms: WebcamTableRooms.active_rooms())
  end
end
