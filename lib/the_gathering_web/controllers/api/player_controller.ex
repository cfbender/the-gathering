defmodule TheGatheringWeb.API.PlayerController do
  use TheGatheringWeb, :controller

  alias TheGathering.Games

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, params), do: render(conn, :index, players: Games.list_players(params))

  def create(conn, %{"player" => attrs}) do
    with {:ok, player} <- Games.create_player(attrs) do
      conn |> put_status(:created) |> render(:show, player: Games.get_player!(player.id))
    end
  end

  def show(conn, %{"id" => id}) do
    case Games.get_player(id) do
      nil -> {:error, :not_found}
      _player -> render(conn, :show, player: Games.get_player!(id))
    end
  end

  # Account and Discord identity are linked by administrators, never by payload.
  @member_attrs ~w(name archived_at)

  def update(conn, %{"id" => id, "player" => attrs}) when is_map(attrs) do
    with player when not is_nil(player) <- Games.get_player(id),
         {:ok, player} <- Games.update_player(player, Map.take(attrs, @member_attrs)) do
      render(conn, :show, player: Games.get_player!(player.id))
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  def update(_conn, _params), do: {:error, :bad_request}

  def merge(conn, %{"id" => id, "target_id" => target_id}) do
    with source when not is_nil(source) <- Games.get_player(id),
         target when not is_nil(target) <- Games.get_player(target_id),
         {:ok, target} <- Games.merge_players(source, target) do
      render(conn, :show, player: Games.get_player!(target.id))
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  def merge(_conn, _params), do: {:error, :bad_request}

  def delete(conn, %{"id" => id}) do
    with player when not is_nil(player) <- Games.get_player(id),
         {:ok, _player} <- Games.delete_player(player) do
      send_resp(conn, :no_content, "")
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end
end
