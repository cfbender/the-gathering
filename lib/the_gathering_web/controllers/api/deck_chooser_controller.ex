defmodule TheGatheringWeb.API.DeckChooserController do
  use TheGatheringWeb, :controller

  alias TheGathering.{Catalog, Games}
  alias TheGatheringWeb.API.DeckJSON

  action_fallback TheGatheringWeb.API.FallbackController

  def show(conn, params) do
    opts = if params["exclude_id"], do: [exclude_id: params["exclude_id"]], else: []

    with {:ok, pick} <- Games.pick_deck(conn.assigns.current_scope.user, opts) do
      card_art =
        if pick.deck, do: Catalog.art_crop_urls(DeckJSON.card_refs(pick.deck)), else: %{}

      render(conn, :show, pick: pick, card_art: card_art)
    end
  end

  def create_outcome(conn, %{"id" => id, "outcome" => outcome})
      when outcome in ["played", "skipped"] do
    with {:ok, deck} <-
           Games.record_deck_outcome(
             conn.assigns.current_scope.user,
             id,
             String.to_existing_atom(outcome)
           ) do
      render(conn, :outcome, deck: deck, outcome: outcome)
    end
  end

  def create_outcome(_conn, _params), do: {:error, :bad_request}

  def sync(conn, _params) do
    with {:ok, counts} <- Games.sync_manavault_decks(conn.assigns.current_scope.user) do
      render(conn, :sync, counts: counts)
    end
  end
end
