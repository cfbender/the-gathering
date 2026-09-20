defmodule TheGatheringWeb.API.AdminDiscordPendingController do
  use TheGatheringWeb, :controller

  alias TheGathering.Discord
  alias TheGatheringWeb.API.AdminDiscordPendingJSON

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, _params) do
    conn
    |> put_view(AdminDiscordPendingJSON)
    |> render(:index, pending_games: Discord.list_pending())
  end

  def update(conn, %{"id" => id, "winner_discord_id" => winner_discord_id}) do
    case Discord.resolve_pending(id, winner_discord_id) do
      {:ok, _report} -> send_resp(conn, :no_content, "")
      {:error, :unknown_game} -> {:error, :not_found}
      {:error, :not_a_player} -> {:error, :bad_request}
      {:error, {:sink_failed, _reason}} -> {:error, :bad_request}
    end
  end

  def update(_conn, _params), do: {:error, :bad_request}

  def delete(conn, %{"id" => id}) do
    case Discord.discard_pending(id) do
      {:ok, _pending} -> send_resp(conn, :no_content, "")
      {:error, :unknown_game} -> {:error, :not_found}
    end
  end
end
