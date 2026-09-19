defmodule TheGatheringWeb.API.DecklistController do
  use TheGatheringWeb, :controller

  alias TheGathering.Decklists

  action_fallback TheGatheringWeb.API.FallbackController

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
