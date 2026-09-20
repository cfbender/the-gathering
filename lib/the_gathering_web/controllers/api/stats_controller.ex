defmodule TheGatheringWeb.API.StatsController do
  use TheGatheringWeb, :controller

  alias TheGathering.Stats

  action_fallback TheGatheringWeb.API.FallbackController

  def overview(conn, params), do: render(conn, :show, stats: Stats.overview(params))

  def player(conn, %{"id" => id} = params) do
    case Stats.player(id, params) do
      nil -> {:error, :not_found}
      stats -> render(conn, :show, stats: stats)
    end
  end

  def commanders(conn, params), do: render(conn, :show, stats: Stats.commanders(params))

  def commander(conn, %{"id" => id} = params) do
    case Stats.commander(id, params) do
      nil -> {:error, :not_found}
      stats -> render(conn, :show, stats: stats)
    end
  end

  def deck(conn, %{"id" => id} = params) do
    case Stats.deck(id, params) do
      nil -> {:error, :not_found}
      stats -> render(conn, :show, stats: stats)
    end
  end
end
