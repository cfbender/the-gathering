defmodule TheGathering.Discord.GameReport do
  @moduledoc "Normalized game data emitted by Discord sources."

  @enforce_keys [
    :external_id,
    :source,
    :played_at,
    :guild_id,
    :channel_id,
    :players,
    :winner_discord_ids,
    :raw
  ]
  defstruct @enforce_keys ++ [details: %{}]

  @type player :: %{
          discord_id: String.t(),
          display_name: String.t(),
          commander_name: String.t() | nil
        }

  @type t :: %__MODULE__{
          external_id: String.t(),
          source: String.t(),
          played_at: DateTime.t(),
          guild_id: String.t(),
          channel_id: String.t(),
          players: [player()],
          winner_discord_ids: [String.t()],
          raw: map(),
          details: map()
        }
end
