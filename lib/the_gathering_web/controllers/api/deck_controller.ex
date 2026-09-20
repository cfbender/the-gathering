defmodule TheGatheringWeb.API.DeckController do
  use TheGatheringWeb, :controller

  alias TheGathering.{Catalog, Games}
  alias TheGatheringWeb.API.DeckJSON

  action_fallback TheGatheringWeb.API.FallbackController

  def index(conn, params) do
    decks = Games.list_decks(params)

    render(conn, :index,
      decks: decks,
      card_art: Catalog.art_crop_urls(DeckJSON.card_refs(decks))
    )
  end

  def create(conn, %{"deck" => attrs}) do
    with {:ok, deck} <- Games.create_deck(attrs) do
      conn |> put_status(:created) |> render_deck(Games.get_deck!(deck.id))
    end
  end

  def show(conn, %{"id" => id}) do
    case Games.get_deck(id) do
      nil -> {:error, :not_found}
      _deck -> render_deck(conn, Games.get_deck!(id))
    end
  end

  def update(conn, %{"id" => id, "deck" => attrs}) do
    with deck when not is_nil(deck) <- Games.get_deck(id),
         {:ok, deck} <- Games.update_deck(deck, attrs) do
      render_deck(conn, Games.get_deck!(deck.id))
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  def delete(conn, %{"id" => id}) do
    with deck when not is_nil(deck) <- Games.get_deck(id),
         {:ok, _deck} <- Games.delete_deck(deck) do
      send_resp(conn, :no_content, "")
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  defp render_deck(conn, deck) do
    render(conn, :show,
      deck: deck,
      card_art: Catalog.art_crop_urls(DeckJSON.card_refs(deck))
    )
  end
end
