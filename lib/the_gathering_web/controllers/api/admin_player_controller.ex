defmodule TheGatheringWeb.API.AdminPlayerController do
  use TheGatheringWeb, :controller

  alias TheGathering.Games

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, params) do
    {players, meta} = Games.list_player_identities(params)
    render(conn, :index, players: players, meta: meta)
  end

  def unlink_identity(conn, %{"id" => id}) do
    with player when not is_nil(player) <- Games.get_player(id),
         {:ok, _player} <- Games.unlink_player_identity(player) do
      send_resp(conn, :no_content, "")
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end
end
