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
    :win_condition,
    :notes,
    :lines,
    :seats,
    :action,
    :target_source,
    :target_external_id,
    :target_portable_id
  ]

  @type t :: %__MODULE__{
          external_id: String.t(),
          game_id: String.t(),
          played_at: DateTime.t(),
          duration_minutes: pos_integer() | nil,
          turns: pos_integer() | nil,
          win_condition: String.t() | nil,
          notes: String.t() | nil,
          lines: [pos_integer()],
          seats: [Seat.t()]
        }
end
