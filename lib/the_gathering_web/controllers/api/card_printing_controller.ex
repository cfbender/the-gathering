defmodule TheGatheringWeb.API.CardPrintingController do
  use TheGatheringWeb, :controller

  alias TheGathering.Catalog

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, params) do
    with {page, ""} when page > 0 <- Integer.parse(Map.get(params, "page", "1")),
         {:ok, printings, has_more} <-
           Catalog.list_printings(params["card_id"], params["name"], page) do
      render(conn, :index, printings: printings, has_more: has_more)
    else
      {:error, _reason} = error -> error
      _invalid -> {:error, :bad_request}
    end
  end

  def show(conn, %{"id" => id}) do
    case Catalog.get_printing(id) do
      nil -> {:error, :not_found}
      printing -> render(conn, :show, printing: printing)
    end
  end
end
