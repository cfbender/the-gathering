defmodule TheGatheringWeb.API.DecklistController do
  use TheGatheringWeb, :controller

  alias TheGathering.{Catalog, Decklists, Games}

  action_fallback TheGatheringWeb.API.FallbackController

  @doc """
  The playable card list behind a deck's linked Moxfield, Archidekt or ManaVault page, with
  catalog type, cost and cached images for each card. 404 when the deck has no supported link
  or the list is missing or private upstream.
  """
  def show(conn, %{"deck_id" => deck_id}) do
    with %{decklist_url: url} when is_binary(url) <- Games.get_deck(deck_id),
         {:ok, decklist} <- Decklists.resolve(url) do
      catalog = Catalog.cards_by_name(Enum.map(decklist.cards, & &1.name))
      render(conn, :cards, decklist: decklist, catalog: catalog)
    else
      {:error, :upstream_error} -> {:error, :bad_gateway}
      _missing -> {:error, :not_found}
    end
  end

  def resolve(conn, %{"url" => url}) when is_binary(url) do
    case Decklists.resolve(url) do
      {:ok, decklist} -> render(conn, :show, decklist: decklist)
      {:error, error} when error in [:invalid_url, :unsupported_url] -> invalid_url()
      {:error, error} when error in [:not_found, :private] -> {:error, :not_found}
      {:error, :upstream_error} -> {:error, :bad_gateway}
    end
  end

  def resolve(_conn, _params), do: invalid_url()

  defp invalid_url do
    changeset =
      {%{}, %{url: :string}}
      |> Ecto.Changeset.cast(%{}, [:url])
      |> Ecto.Changeset.add_error(:url, "is not a supported deck-list URL")

    {:error, changeset}
  end
end
