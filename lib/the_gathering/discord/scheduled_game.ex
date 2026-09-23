defmodule TheGathering.Discord.ScheduledGame do
  @moduledoc false
  use Ecto.Schema
  import Ecto.Changeset

  schema "discord_scheduled_games" do
    field :guild_id, :string
    field :channel_id, :string
    field :message_id, :string
    field :host_discord_id, :string
    field :title, :string, default: "Commander game"
    field :format, :string
    field :start_at, :utc_datetime
    field :min_players, :integer, default: 3
    field :status, :string, default: "open"
    field :room_id, Ecto.UUID
    field :players, :map, default: %{}
    field :announcement_id, :string
    field :message_dirty, :boolean, default: true
    timestamps(type: :utc_datetime)
  end

  def changeset(game, attrs) do
    game
    |> cast(attrs, [:title, :format, :start_at, :min_players])
    |> validate_required([:title, :min_players])
    |> validate_length(:title, max: 100)
    |> validate_length(:format, max: 100)
    |> validate_number(:min_players, greater_than_or_equal_to: 2, less_than_or_equal_to: 10)
  end
end
