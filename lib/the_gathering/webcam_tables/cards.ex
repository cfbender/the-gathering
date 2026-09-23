defmodule TheGathering.WebcamTables.Cards do
  @moduledoc false

  def update(cards, %{"type" => "card_identified", "entry" => entry}, seats) do
    if valid?(entry) and
         Enum.any?(seats, &(&1.peer_id == entry["ownerPeerId"] and is_nil(&1.reveal_to))) do
      cards =
        Enum.uniq_by(
          cards ++ [entry],
          &{&1["ownerPeerId"], String.downcase(String.trim(&1["card"]["name"]))}
        )

      {:ok, Enum.take(cards, 500)}
    else
      :error
    end
  end

  def update(cards, %{"type" => "card_removed", "id" => id}, _seats) when is_binary(id),
    do: {:ok, Enum.reject(cards, &(&1["id"] == id))}

  def update(cards, %{"type" => "cards_cleared", "ownerPeerId" => id}, _seats) when is_binary(id),
    do: {:ok, Enum.reject(cards, &(&1["ownerPeerId"] == id))}

  def update(_cards, _payload, _seats), do: :error

  defp valid?(
         %{
           "id" => id,
           "ownerPeerId" => owner,
           "byPlayerName" => by,
           "at" => at,
           "card" => %{"id" => art, "name" => name, "set" => set}
         } = entry
       ) do
    is_integer(at) and
      Enum.all?(
        [id, owner, by, art, name, set],
        &(is_binary(&1) and byte_size(&1) in 1..300)
      ) and byte_size(Jason.encode!(entry)) <= 2048
  end

  defp valid?(_entry), do: false
end
