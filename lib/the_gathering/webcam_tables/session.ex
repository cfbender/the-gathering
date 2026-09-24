defmodule TheGathering.WebcamTables.Session do
  @moduledoc """
  Versioned server-owned snapshots, retained for seven days after last activity
  or until their room closes as idle.
  JSON decoding uses explicit field lists, never dynamically created atoms.
  Writes finish before an action is acknowledged, including rapid life changes.
  """
  use Ecto.Schema
  import Ecto.Query

  alias TheGathering.Repo

  @version 2

  @primary_key {:id, :string, autogenerate: false}
  schema "webcam_table_sessions" do
    field :snapshot, :binary
    field :expires_at, :utc_datetime_usec
  end

  def load(id) do
    with %__MODULE__{snapshot: data, expires_at: expires} <- Repo.get(__MODULE__, id),
         true <- DateTime.after?(expires, DateTime.utc_now()),
         %{"version" => @version, "state" => entry} <- Jason.decode!(data) do
      restore(entry)
    else
      # Missing, expired, or an older snapshot format: start a fresh table.
      _other -> nil
    end
  end

  def save(id, entry) do
    Repo.insert!(
      %__MODULE__{
        id: id,
        snapshot: Jason.encode!(%{version: @version, state: entry}),
        expires_at: DateTime.add(DateTime.utc_now(), 7, :day)
      },
      on_conflict: {:replace, [:snapshot, :expires_at]},
      conflict_target: :id
    )

    :ok
  end

  def delete(id) do
    Repo.delete_all(from session in __MODULE__, where: session.id == ^id)
    :ok
  end

  def prune do
    now = DateTime.utc_now()
    Repo.delete_all(from session in __MODULE__, where: session.expires_at < ^now)
  end

  defp restore(data) do
    turns =
      fields(data["turns"], [
        :active_player_id,
        :counts,
        :elapsed_ms,
        :started_elapsed_ms,
        :revision
      ])

    %{
      timer: fields(data["timer"], [:started_at, :paused_at, :paused_ms]),
      peer_ids: data["peer_ids"],
      owner_id: data["owner_id"],
      auto_randomize: data["auto_randomize"],
      mode: data["mode"],
      team_life: player_keys(data["team_life"]),
      monarch: data["monarch"] && fields(data["monarch"], [:peer_id, :player_name]),
      monarch_revision: data["monarch_revision"],
      cards: data["cards"],
      all_seats: restore_seats(data["all_seats"]),
      eliminated_seats: restore_seats(data["eliminated_seats"]),
      turns: %{
        turns
        | counts: player_keys(turns.counts),
          elapsed_ms: player_keys(turns.elapsed_ms)
      }
    }
  end

  defp restore_seats(seats) do
    Map.new(seats, fn {id, seat} ->
      {String.to_integer(id),
       fields(seat, [
         :peer_id,
         :player_id,
         :player_name,
         :joined_at,
         :life,
         :camera_off,
         :poison,
         :rad,
         :commander_casts,
         :commander_damage,
         :reveal_to,
         :eliminated,
         :spectator,
         :deck_id,
         :deck_name
       ])}
    end)
  end

  defp player_keys(values),
    do: Map.new(values, fn {id, value} -> {String.to_integer(id), value} end)

  defp fields(data, keys) do
    for key <- keys,
        Map.has_key?(data, Atom.to_string(key)),
        into: %{},
        do: {key, data[Atom.to_string(key)]}
  end
end
