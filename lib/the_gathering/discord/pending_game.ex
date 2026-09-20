defmodule TheGathering.Discord.PendingGame do
  @moduledoc "Durable staging for winnerless Discord game reports."

  use Ecto.Schema

  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Games.Game

  schema "pending_discord_games" do
    field :external_id, :string
    field :guild_id, :string
    field :channel_id, :string
    field :played_at, :utc_datetime
    field :players, :map
    field :raw, :map, default: %{}

    timestamps(type: :utc_datetime)
  end

  def changeset(pending_game, attrs) do
    pending_game
    |> cast(attrs, [:external_id, :guild_id, :channel_id, :played_at, :players, :raw])
    |> validate_required([:external_id, :guild_id, :channel_id, :played_at, :players, :raw])
    |> unique_constraint(:external_id)
  end

  def ordered_query do
    from pending in winnerless_query(), order_by: [desc: pending.played_at, desc: pending.id]
  end

  def by_external_id_query(external_id) do
    from pending in __MODULE__, where: pending.external_id == ^external_id
  end

  def latest_in_channel_query(channel_id) do
    from pending in winnerless_query(),
      where: pending.channel_id == ^channel_id,
      order_by: [desc: pending.played_at, desc: pending.id],
      limit: 1
  end

  def expired_query(cutoff) do
    from pending in __MODULE__, where: pending.updated_at < ^cutoff
  end

  defp winnerless_query do
    from pending in __MODULE__,
      as: :pending,
      where:
        not exists(
          from game in Game,
            where:
              game.source == "discord" and
                game.external_id == parent_as(:pending).external_id
        )
  end
end
