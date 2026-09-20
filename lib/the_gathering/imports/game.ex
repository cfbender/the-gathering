defmodule TheGathering.Imports.Game do
  @moduledoc "Normalized game produced by every import parser."

  alias TheGathering.Imports.Seat

  @enforce_keys [:external_id, :game_id, :played_at, :lines, :seats]
  defstruct [
    :external_id,
    :game_id,
    :played_at,
    :duration_minutes,
    :turns,
    :notes,
    :lines,
    :seats
  ]

  @type t :: %__MODULE__{
          external_id: String.t(),
          game_id: String.t(),
          played_at: DateTime.t(),
          duration_minutes: pos_integer() | nil,
          turns: pos_integer() | nil,
          notes: String.t() | nil,
          lines: [pos_integer()],
          seats: [Seat.t()]
        }
end
