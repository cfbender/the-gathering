defmodule TheGatheringWeb.API.GameController do
  use TheGatheringWeb, :controller

  alias TheGathering.Games

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, params) do
    {games, pagination} = Games.list_games(params)
    render(conn, :index, games: games, pagination: pagination)
  end

  def create(conn, %{"game" => attrs}) do
    with {:ok, game} <- Games.create_game(attrs) do
      conn |> put_status(:created) |> render(:show, game: game)
    end
  end

  def show(conn, %{"id" => id}) do
    case Games.get_game(id) do
      nil -> {:error, :not_found}
      _game -> render(conn, :show, game: Games.get_game!(id))
    end
  end

  def update(conn, %{"id" => id, "game" => attrs}) do
    with game when not is_nil(game) <- Games.get_game(id),
         {:ok, game} <- Games.update_game(game, attrs) do
      render(conn, :show, game: game)
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  def delete(conn, %{"id" => id}) do
    with game when not is_nil(game) <- Games.get_game(id),
         {:ok, _game} <- Games.delete_game(game) do
      send_resp(conn, :no_content, "")
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end
end
