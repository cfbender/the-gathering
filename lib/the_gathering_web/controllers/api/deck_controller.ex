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
    with :ok <- authorize_owner(conn, attrs),
         {:ok, deck} <- Games.create_deck(attrs) do
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
         :ok <- authorize(conn, deck),
         {:ok, deck} <- Games.update_deck(deck, attrs) do
      render_deck(conn, Games.get_deck!(deck.id))
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  # `replacement_deck_id` moves the deck's games to another of the player's decks;
  # without it those seats keep no deck.
  def delete(conn, %{"id" => id} = params) do
    with deck when not is_nil(deck) <- Games.get_deck(id),
         :ok <- authorize(conn, deck),
         {:ok, replacement} <- replacement_deck(params["replacement_deck_id"]),
         {:ok, _deck} <- Games.delete_deck(deck, replacement) do
      send_resp(conn, :no_content, "")
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  defp replacement_deck(id) when id in [nil, ""], do: {:ok, nil}

  defp replacement_deck(id) do
    case Games.get_deck(id) do
      nil -> {:error, :bad_request}
      deck -> {:ok, deck}
    end
  end

  defp authorize(conn, deck) do
    if Games.can_manage_deck?(conn.assigns.current_scope.user, deck),
      do: :ok,
      else: {:error, :forbidden}
  end

  # Creating a deck for another member's player is refused; an unknown player id
  # falls through to the changeset's foreign-key error.
  defp authorize_owner(conn, attrs) do
    case Ecto.Type.cast(:id, attrs["player_id"]) do
      {:ok, nil} -> :ok
      {:ok, player_id} -> authorize_player(conn, Games.get_player(player_id))
      :error -> :ok
    end
  end

  defp authorize_player(_conn, nil), do: :ok

  defp authorize_player(conn, player) do
    if Games.can_manage_player?(conn.assigns.current_scope.user, player),
      do: :ok,
      else: {:error, :forbidden}
  end

  defp render_deck(conn, deck) do
    render(conn, :show,
      deck: deck,
      card_art: Catalog.art_crop_urls(DeckJSON.card_refs(deck))
    )
  end
end
