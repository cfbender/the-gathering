defmodule TheGathering.Decklists.Decklist do
  @moduledoc "Public metadata resolved from a supported deck-list service."

  @enforce_keys [:source, :id, :url, :name, :commanders, :fetched_at]
  @type t :: %__MODULE__{
          source: :moxfield | :archidekt | :manavault,
          id: String.t(),
          url: String.t(),
          name: String.t(),
          commanders: [%{name: String.t()}],
          color_identity: [String.t()] | nil,
          author: String.t() | nil,
          card_count: non_neg_integer() | nil,
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
    commanders: []
  ]
end
