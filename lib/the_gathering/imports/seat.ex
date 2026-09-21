defmodule TheGathering.Imports.Seat do
  @moduledoc "Normalized seat produced by every import parser."

  @enforce_keys [:line, :player, :deck, :commander, :seat, :result]
  defstruct [
    :line,
    :player,
    :discord_id,
    :deck,
    :commander,
    :commander_card_id,
    :partner_name,
    :partner_card_id,
    :color_identity,
    :decklist_url,
    :seat,
    :result,
    :kills,
    :mvp_card,
    :mvp_card_id
  ]

  @type t :: %__MODULE__{
          line: pos_integer(),
          player: String.t(),
          discord_id: String.t() | nil,
          deck: String.t(),
          commander: String.t(),
          commander_card_id: String.t() | nil,
          partner_name: String.t() | nil,
          partner_card_id: String.t() | nil,
          color_identity: String.t() | nil,
          decklist_url: String.t() | nil,
          seat: pos_integer(),
          result: String.t(),
          mvp_card: String.t() | nil,
          mvp_card_id: String.t() | nil
        }
end
