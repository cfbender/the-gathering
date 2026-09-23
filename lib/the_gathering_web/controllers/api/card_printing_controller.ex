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

  def details(conn, %{"id" => id}) do
    with {:ok, details} <- Catalog.printing_details(id) do
      conn
      |> put_resp_header("cache-control", "private, max-age=86400")
      |> render(:details, details: details)
    end
  end

  def rulings(conn, %{"id" => id}) do
    with {:ok, rulings} <- Catalog.printing_rulings(id) do
      render(conn, :rulings, rulings: rulings)
    end
  end
end
