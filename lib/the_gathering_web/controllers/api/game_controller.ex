defmodule TheGatheringWeb.API.GameController do
  use TheGatheringWeb, :controller

  alias TheGathering.{Catalog, Games}
  alias TheGatheringWeb.API.GameJSON

  action_fallback TheGatheringWeb.API.FallbackController

  @member_attrs ~w(played_at duration_minutes turns notes seats)

  def index(conn, params) do
    {games, pagination} = Games.list_games(params)

    render(conn, :index,
      games: games,
      pagination: pagination,
      card_art: Catalog.art_crop_urls(GameJSON.card_refs(games))
    )
  end

  def create(conn, %{"game" => attrs}) when is_map(attrs) do
    with {:ok, game} <-
           Games.create_game(Map.take(attrs, @member_attrs), conn.assigns.current_scope.user.id) do
      conn |> put_status(:created) |> render_game(game)
    end
  end

  def create(_conn, _params), do: {:error, :bad_request}

  def show(conn, %{"id" => id}) do
    case Games.get_game(id) do
      nil -> {:error, :not_found}
      _game -> render_game(conn, Games.get_game!(id))
    end
  end

  def update(conn, %{"id" => id, "game" => attrs}) when is_map(attrs) do
    with game when not is_nil(game) <- Games.get_game(id),
         :ok <- authorize(conn, game),
         {:ok, game} <- Games.update_game(game, Map.take(attrs, @member_attrs)) do
      render_game(conn, game)
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  def update(_conn, _params), do: {:error, :bad_request}

  def delete(conn, %{"id" => id}) do
    with game when not is_nil(game) <- Games.get_game(id),
         :ok <- authorize(conn, game),
         {:ok, _game} <- Games.delete_game(game) do
      send_resp(conn, :no_content, "")
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  defp authorize(conn, game) do
    if Games.can_manage_game?(conn.assigns.current_scope.user, game),
      do: :ok,
      else: {:error, :forbidden}
  end

  defp render_game(conn, game) do
    render(conn, :show,
      game: game,
      card_art: Catalog.art_crop_urls(GameJSON.card_refs(game))
    )
  end
end
