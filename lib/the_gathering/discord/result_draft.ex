defmodule TheGathering.Discord.ResultDraft do
  @moduledoc false
  use Ecto.Schema

  @primary_key {:id, :binary_id, autogenerate: true}
  schema "discord_result_drafts" do
    belongs_to :pending_game, TheGathering.Discord.PendingGame
    field :discord_id, :string
    field :guild_id, :string
    field :channel_id, :string
    field :snapshot, :binary
    field :data, :map, default: %{}
    field :expires_at, :utc_datetime
  end
end
