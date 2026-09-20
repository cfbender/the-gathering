defmodule TheGatheringWeb.API.CardController do
  use TheGatheringWeb, :controller

  alias TheGathering.Catalog

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, params) do
    cards =
      Catalog.search(Map.get(params, "q", ""),
        commander: boolean_param(params["commander"]),
        partner: boolean_param(params["partner"]),
        limit: limit_param(params["limit"])
      )

    render(conn, :index, cards: cards)
  end

  def show(conn, %{"id" => id}) do
    case Catalog.get_card(id) do
      nil -> {:error, :not_found}
      card -> render(conn, :show, card: card)
    end
  end

  defp boolean_param("true"), do: true
  defp boolean_param("false"), do: false
  defp boolean_param(_value), do: nil

  defp limit_param(value) when is_binary(value) do
    case Integer.parse(value) do
      {limit, ""} -> limit
      _error -> 20
    end
  end

  defp limit_param(_value), do: 20
end
