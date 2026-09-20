defmodule TheGathering.Discord.PendingGame do
  @moduledoc "Durable staging for winnerless Discord game reports."

  use Ecto.Schema

  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Discord.GameReport
  alias TheGathering.Repo

  @retention_days 30

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

  @doc "Stages a report, replacing normalized data when SpellBot edits the same game."
  def upsert(%GameReport{} = report) do
    prune_stale()

    attrs = %{
      external_id: report.external_id,
      guild_id: report.guild_id,
      channel_id: report.channel_id,
      played_at: report.played_at,
      players: %{"seats" => encode_players(report.players)},
      raw: stringify_keys(report.raw)
    }

    %__MODULE__{}
    |> changeset(attrs)
    |> Repo.insert(
      conflict_target: :external_id,
      on_conflict: {:replace, [:guild_id, :channel_id, :played_at, :players, :raw, :updated_at]}
    )
  end

  def list do
    prune_stale()
    Repo.all(from pending in __MODULE__, order_by: [desc: pending.played_at, desc: pending.id])
  end

  def get_by_external_id(external_id), do: Repo.get_by(__MODULE__, external_id: external_id)
  def get(id), do: Repo.get(__MODULE__, id)

  def latest_in_channel(channel_id) do
    prune_stale()

    Repo.one(
      from pending in __MODULE__,
        where: pending.channel_id == ^channel_id,
        order_by: [desc: pending.played_at, desc: pending.id],
        limit: 1
    )
  end

  def delete(%__MODULE__{} = pending), do: Repo.delete(pending)

  def prune_stale(now \\ DateTime.utc_now()) do
    cutoff = DateTime.add(now, -@retention_days, :day)
    Repo.delete_all(from pending in __MODULE__, where: pending.updated_at < ^cutoff)
    :ok
  end

  def to_report(%__MODULE__{} = pending) do
    %GameReport{
      external_id: pending.external_id,
      source: "discord",
      played_at: pending.played_at,
      guild_id: pending.guild_id,
      channel_id: pending.channel_id,
      players: decode_players(pending.players["seats"]),
      winner_discord_ids: [],
      raw: pending.raw
    }
  end

  defp encode_players(players) do
    Enum.map(players, fn player ->
      %{
        "discord_id" => player.discord_id,
        "display_name" => player.display_name,
        "commander_name" => player.commander_name
      }
    end)
  end

  defp decode_players(players) do
    Enum.map(players, fn player ->
      %{
        discord_id: player["discord_id"],
        display_name: player["display_name"],
        commander_name: player["commander_name"]
      }
    end)
  end

  defp stringify_keys(value) when is_map(value) do
    Map.new(value, fn {key, nested} -> {to_string(key), stringify_keys(nested)} end)
  end

  defp stringify_keys(value) when is_list(value), do: Enum.map(value, &stringify_keys/1)
  defp stringify_keys(value), do: value
end
