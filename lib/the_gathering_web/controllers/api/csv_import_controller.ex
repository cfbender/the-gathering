defmodule TheGatheringWeb.API.CSVImportController do
  use TheGatheringWeb, :controller

  alias TheGathering.Imports

  action_fallback TheGatheringWeb.API.FallbackController

  @sample """
  game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes
  friday-001,2026-09-18,Alice,Birds of a Feather,"Kangee, Sky Warden",1,win,Swan Song,75,10,Friday Commander
  friday-001,2026-09-18,Bob,Goblins,Krenko Mob Boss,2,loss,,75,10,Friday Commander
  """

  def sample(conn, _params) do
    send_download(conn, {:binary, @sample},
      filename: "the-gathering-games.csv",
      content_type: "text/csv"
    )
  end

  def preview(conn, %{"csv" => csv}) when is_binary(csv) do
    render(conn, :preview, preview: Imports.preview_csv(csv))
  end

  def preview(_conn, _params), do: {:error, :bad_request}

  def create(conn, %{"csv" => csv}) when is_binary(csv) do
    user_id = conn.assigns.current_scope.user.id

    case Imports.import_csv(csv, user_id) do
      {:ok, result} ->
        render(conn, :result, result: result)

      {:error, {:validation, preview}} ->
        conn |> put_status(:unprocessable_entity) |> render(:preview, preview: preview)

      {:error, {:changeset, changeset}} ->
        {:error, changeset}
    end
  end

  def create(_conn, _params), do: {:error, :bad_request}
end
