defmodule TheGatheringWeb.API.DiscordResultDraftController do
  use TheGatheringWeb, :controller

  alias TheGathering.Catalog
  alias TheGathering.Discord.WebGameDraft
  alias TheGatheringWeb.API.GameJSON

  action_fallback TheGatheringWeb.API.FallbackController

  def show(conn, %{"id" => id}) do
    with {:ok, draft} <- WebGameDraft.preview(id, conn.assigns.current_scope.user) do
      render(conn, :show, draft: draft)
    end
  end

  def create(conn, %{"id" => id, "game" => attrs}) when is_map(attrs) do
    with {:ok, game} <- WebGameDraft.save(id, conn.assigns.current_scope.user, attrs) do
      conn
      |> put_status(:created)
      |> put_view(json: GameJSON)
      |> render(:show, game: game, card_art: Catalog.art_crop_urls(GameJSON.card_refs(game)))
    end
  end

  def create(_conn, _params), do: {:error, :bad_request}
end
