defmodule TheGatheringWeb.API.AdminUserController do
  use TheGatheringWeb, :controller

  alias TheGathering.{Accounts, Catalog, Games}
  alias TheGatheringWeb.API.{PlayerJSON, UserJSON}

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, _params) do
    conn |> put_view(UserJSON) |> render(:index, users: Accounts.list_users())
  end

  def update(conn, %{"id" => id, "user" => attrs}) do
    with {:ok, user} <- fetch_user(id),
         {:ok, user} <- Accounts.update_user(user, admin_update_attrs(attrs)) do
      conn |> put_view(UserJSON) |> render(:show, user: user)
    end
  end

  def update(_conn, _params), do: {:error, :bad_request}

  def link_player(conn, %{"id" => id, "player_id" => player_id}) do
    with {:ok, user} <- fetch_user(id),
         player when not is_nil(player) <- Games.get_player(player_id),
         {:ok, player} <- Games.link_player_to_user(player, user) do
      player = Games.get_player!(player.id)

      conn
      |> put_view(PlayerJSON)
      |> render(:show,
        player: player,
        card_art: Catalog.art_crop_urls(PlayerJSON.card_refs(player))
      )
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  def link_player(_conn, _params), do: {:error, :bad_request}

  def revoke_sessions(conn, %{"id" => id}) do
    with {:ok, user} <- fetch_user(id),
         {:ok, user} <- Accounts.revoke_all_sessions(user) do
      conn |> put_view(UserJSON) |> render(:show, user: user)
    end
  end

  def delete(conn, %{"id" => id}) do
    with {:ok, user} <- fetch_user(id),
         {:ok, _user} <- Accounts.delete_user(user, conn.assigns.current_scope.user) do
      send_resp(conn, :no_content, "")
    end
  end

  defp fetch_user(id) do
    case Accounts.get_user(id) do
      nil -> {:error, :not_found}
      user -> {:ok, user}
    end
  end

  defp admin_update_attrs(attrs) do
    case Map.fetch(attrs, "disabled") do
      {:ok, true} ->
        attrs
        |> Map.delete("disabled")
        |> Map.put("disabled_at", DateTime.utc_now() |> DateTime.truncate(:second))

      {:ok, false} ->
        attrs |> Map.delete("disabled") |> Map.put("disabled_at", nil)

      :error ->
        attrs
    end
  end
end
