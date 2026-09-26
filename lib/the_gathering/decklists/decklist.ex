defmodule TheGathering.Decklists.Decklist do
  @moduledoc "Public metadata resolved from a supported deck-list service."

  @enforce_keys [:source, :id, :url, :name, :commanders, :fetched_at]

  @typedoc """
  One entry of the playable list (commander zone and main deck; maybe-, side- and
  considering boards are left out). `printing_id` is the exact Scryfall printing the list
  names, when the service records one.
  """
  @type card :: %{
          name: String.t(),
          quantity: pos_integer(),
          zone: :commander | :mainboard,
          printing_id: String.t() | nil
        }

  @type t :: %__MODULE__{
          source: :moxfield | :archidekt | :manavault,
          id: String.t(),
          url: String.t(),
          name: String.t(),
          commanders: [%{name: String.t()}],
          color_identity: [String.t()] | nil,
          author: String.t() | nil,
          card_count: non_neg_integer() | nil,
          cards: [card()],
          fetched_at: DateTime.t()
        }

  defstruct [
    :source,
    :id,
    :url,
    :name,
    :color_identity,
    :author,
    :card_count,
    :fetched_at,
    commanders: [],
    cards: []
  ]

  @doc "Builds a card entry, or `nil` when the upstream entry has no usable name."
  def card(name, quantity, zone, printing_id) when is_binary(name) and name != "" do
    %{
      name: name,
      quantity: if(is_integer(quantity) and quantity > 0, do: quantity, else: 1),
      zone: zone,
      printing_id: if(is_binary(printing_id) and printing_id != "", do: printing_id)
    }
  end

  def card(_name, _quantity, _zone, _printing_id), do: nil
end
