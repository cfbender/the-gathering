defmodule TheGathering.Games.Summary do
  @moduledoc false

  import Ecto.Query

  alias TheGathering.Games.Game
  alias TheGathering.Repo

  def find(reference) do
    reference = String.trim(reference)

    cond do
      reference == "" ->
        fetch(from g in Game, order_by: [desc: g.played_at, desc: g.id], limit: 1)

      Regex.match?(~r/^#?SB[0-9]{1,20}$/i, reference) ->
        external_id = "spellbot:" <> (reference |> String.trim_leading("#") |> String.upcase())
        fetch(from g in Game, where: g.source == "discord" and g.external_id == ^external_id)

      Regex.match?(~r/^[0-9]{1,18}$/, reference) ->
        id = String.to_integer(reference)
        fetch(from g in Game, where: g.id == ^id)

      true ->
        {:error, :bad_request}
    end
  end

  defp fetch(query) do
    case Repo.one(preload(query, seats: [:player, :deck])) do
      nil -> {:error, :not_found}
      game -> {:ok, game}
    end
  end
end
