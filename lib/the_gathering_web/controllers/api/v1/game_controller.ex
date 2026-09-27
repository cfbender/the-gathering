defmodule TheGatheringWeb.API.V1.GameController do
  @moduledoc """
  Game history for API-key clients, newest first.

  Accepts `player_id` (an ID, or `me` for the key owner's linked player),
  `date_from`/`date_to` (inclusive ISO dates read in `tz`, default UTC), and
  `page`/`per_page` (at most 100). Invalid filter values are rejected rather than
  ignored so a typo never silently widens the result to every game.
  """
  use TheGatheringWeb, :controller

  alias TheGathering.{Catalog, Games, LocalTime}
  alias TheGatheringWeb.API.GameJSON

  action_fallback TheGatheringWeb.API.FallbackController

  @filters ~w(player_id date_from date_to tz page per_page)

  def index(conn, params) do
    with {:ok, filters} <- filters(conn.assigns.current_scope.user, Map.take(params, @filters)) do
      {games, pagination} = Games.list_games(filters)

      conn
      |> put_view(GameJSON)
      |> render(:index,
        games: games,
        pagination: pagination,
        card_art: Catalog.art_crop_urls(GameJSON.card_refs(games))
      )
    end
  end

  defp filters(user, params) do
    with {:ok, params} <- resolve_player(user, params),
         :ok <- validate_integer(params, "player_id"),
         :ok <- validate_integer(params, "page"),
         :ok <- validate_integer(params, "per_page"),
         :ok <- validate_date(params, "date_from"),
         :ok <- validate_date(params, "date_to"),
         :ok <- validate_zone(params) do
      {:ok, params}
    end
  end

  # `LocalTime.zone/1` falls back to UTC, which would quietly shift the date window.
  defp validate_zone(%{"tz" => zone}) do
    if is_binary(zone) and LocalTime.zone(zone) == zone, do: :ok, else: {:error, :bad_request}
  end

  defp validate_zone(_params), do: :ok

  defp resolve_player(user, %{"player_id" => "me"} = params) do
    case Games.get_player_for_user(user) do
      nil -> {:error, :not_found}
      player -> {:ok, Map.put(params, "player_id", Integer.to_string(player.id))}
    end
  end

  defp resolve_player(_user, params), do: {:ok, params}

  defp validate_integer(params, key) do
    case Map.fetch(params, key) do
      :error ->
        :ok

      {:ok, value} when is_binary(value) ->
        case Integer.parse(value) do
          {integer, ""} when integer > 0 -> :ok
          _invalid -> {:error, :bad_request}
        end

      {:ok, _value} ->
        {:error, :bad_request}
    end
  end

  defp validate_date(params, key) do
    case Map.fetch(params, key) do
      :error ->
        :ok

      {:ok, value} when is_binary(value) ->
        case Date.from_iso8601(value) do
          {:ok, _date} -> :ok
          {:error, _reason} -> {:error, :bad_request}
        end

      {:ok, _value} ->
        {:error, :bad_request}
    end
  end
end
