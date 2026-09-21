defmodule TheGathering.Games.DeleteDeck do
  @moduledoc false

  import Ecto.Query

  alias TheGathering.Games.{Deck, GamePlayer}
  alias TheGathering.Repo

  @doc """
  Deletes a deck. Seats that used it move to `replacement`, a deck of the same player,
  or lose their deck when no replacement is given.
  """
  def run(%Deck{} = deck, replacement) do
    Repo.transaction(fn ->
      case ensure_replaceable(deck, replacement) do
        :ok ->
          reassign_seats(deck, replacement)
          Repo.delete!(deck)

        {:error, reason} ->
          Repo.rollback(reason)
      end
    end)
  end

  defp ensure_replaceable(_deck, nil), do: :ok
  defp ensure_replaceable(%Deck{id: id}, %Deck{id: id}), do: {:error, :bad_request}

  defp ensure_replaceable(%Deck{player_id: player_id}, %Deck{player_id: player_id}), do: :ok
  defp ensure_replaceable(_deck, _replacement), do: {:error, :bad_request}

  defp reassign_seats(deck, replacement) do
    from(seat in GamePlayer, where: seat.deck_id == ^deck.id)
    |> Repo.update_all(set: [deck_id: replacement && replacement.id])
  end
end
