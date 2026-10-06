defmodule TheGathering.Discord.ScheduledGames do
  @moduledoc "Durable queue transitions. SQLite immediate transactions serialize roster changes."
  import Ecto.Query
  alias TheGathering.Discord.ScheduledGame
  alias TheGathering.Repo

  # How long an underfilled game stays open after pinging its maybe list.
  @maybe_grace_seconds 15 * 60
  @max_listed 10

  def maybe_grace_seconds, do: @maybe_grace_seconds

  def create(attrs, actor) do
    with :ok <- authorize(actor) do
      %ScheduledGame{
        guild_id: actor.guild_id,
        channel_id: actor.channel_id,
        host_discord_id: actor.discord_id
      }
      |> ScheduledGame.changeset(attrs)
      |> Repo.insert()
    end
  end

  def attach_message(id, message_id) do
    Repo.get!(ScheduledGame, id)
    |> Ecto.Changeset.change(message_id: to_string(message_id))
    |> Repo.update!()
  end

  def cancel_unpublished(id) do
    Repo.update_all(from(g in ScheduledGame, where: g.id == ^id and is_nil(g.message_id)),
      set: [status: "cancelled", message_dirty: false]
    )
  end

  @doc "Read-only check used before opening the change-time modal; `act/4` re-checks on submit."
  def manageable?(id, actor) do
    with :ok <- authorize(actor),
         %ScheduledGame{status: "open"} = game <- Repo.get(ScheduledGame, id),
         true <- same_message?(game, actor) do
      host_or_admin?(game, actor)
    else
      _ -> false
    end
  end

  def act(id, action, actor, now) do
    Repo.transaction(fn ->
      with :ok <- authorize(actor),
           %ScheduledGame{} = game <- Repo.get(ScheduledGame, id),
           true <- same_message?(game, actor),
           %ScheduledGame{status: "open"} = game <- settle(game, now),
           {:ok, changes} <- changes(game, action, actor, now) do
        game |> Ecto.Changeset.change(changes) |> Repo.update!() |> settle(now)
      else
        %ScheduledGame{} = game -> game
        {:error, reason} -> Repo.rollback(reason)
        _ -> Repo.rollback(:forbidden)
      end
    end)
  end

  def settle(id, now) when is_integer(id) do
    Repo.transaction(fn -> Repo.get!(ScheduledGame, id) |> settle(now) end)
  end

  def settle(%ScheduledGame{status: "open", message_id: message} = game, now)
      when not is_nil(message) do
    enough? = map_size(game.players) >= game.min_players
    due? = game.start_at != nil and DateTime.compare(game.start_at, now) != :gt

    cond do
      due? -> settle_due(game, enough?, now)
      game.start_at == nil and enough? -> transition(game, "started", now)
      true -> game
    end
  end

  def settle(game, _now), do: game

  def pending_ids(now, after_id \\ 0) do
    Repo.all(
      from g in ScheduledGame,
        where:
          g.id > ^after_id and not is_nil(g.message_id) and
            (g.message_dirty or
               (g.status == "open" and g.start_at <= ^now)),
        order_by: [asc: g.id],
        limit: 100,
        select: g.id
    )
  end

  defp transition(game, status, now) do
    room_id = if status == "started", do: Ecto.UUID.generate()
    # This compare-and-set is the final guard even if two workers observe the same queue.
    Repo.update_all(from(g in ScheduledGame, where: g.id == ^game.id and g.status == "open"),
      set: [
        status: status,
        room_id: room_id,
        message_dirty: true,
        updated_at: DateTime.truncate(now, :second)
      ]
    )

    Repo.get!(ScheduledGame, game.id)
  end

  # Maybes never count toward the minimum. An underfilled game pings them once at its
  # start time and stays open for the grace period, starting as soon as it fills.
  defp settle_due(game, true, now), do: transition(game, "started", now)
  defp settle_due(%{maybe_pinged_at: %DateTime{}} = game, _, now), do: await_maybe(game, now)

  defp settle_due(game, _, now) when map_size(game.maybe) > 0, do: ping_maybe(game, now)
  defp settle_due(game, _, now), do: transition(game, "expired", now)

  defp ping_maybe(game, now) do
    Repo.update_all(
      from(g in ScheduledGame,
        where: g.id == ^game.id and g.status == "open" and is_nil(g.maybe_pinged_at)
      ),
      set: [
        maybe_pinged_at: DateTime.truncate(now, :second),
        message_dirty: true,
        updated_at: DateTime.truncate(now, :second)
      ]
    )

    Repo.get!(ScheduledGame, game.id)
  end

  defp await_maybe(game, now) do
    deadline = DateTime.add(game.maybe_pinged_at, @maybe_grace_seconds)
    if DateTime.compare(deadline, now) == :gt, do: game, else: transition(game, "expired", now)
  end

  defp changes(game, "join", actor, now) do
    if full?(game.players, actor) do
      {:error, :full}
    else
      {:ok,
       [
         players: put_entry(game.players, actor, now),
         maybe: Map.delete(game.maybe, actor.discord_id),
         message_dirty: true
       ]}
    end
  end

  defp changes(game, "maybe", actor, now) do
    if full?(game.maybe, actor) do
      {:error, :maybe_full}
    else
      {:ok,
       [
         maybe: put_entry(game.maybe, actor, now),
         players: Map.delete(game.players, actor.discord_id),
         message_dirty: true
       ]}
    end
  end

  defp changes(game, "leave", actor, _now),
    do:
      {:ok,
       [
         players: Map.delete(game.players, actor.discord_id),
         maybe: Map.delete(game.maybe, actor.discord_id),
         message_dirty: true
       ]}

  defp changes(game, "cancel", actor, _now) do
    if host_or_admin?(game, actor),
      do: {:ok, [status: "cancelled", message_dirty: true]},
      else: {:error, :forbidden}
  end

  defp changes(game, {"time", start_at}, actor, _now) do
    if host_or_admin?(game, actor),
      do:
        {:ok, [start_at: start_at, maybe_pinged_at: nil, maybe_ping_id: nil, message_dirty: true]},
      else: {:error, :forbidden}
  end

  defp changes(_game, _action, _actor, _now), do: {:error, :forbidden}

  defp full?(list, actor),
    do: map_size(list) >= @max_listed and not Map.has_key?(list, actor.discord_id)

  defp put_entry(list, actor, now) do
    entry = Map.get(list, actor.discord_id, %{"joined_at" => DateTime.to_iso8601(now)})
    Map.put(list, actor.discord_id, Map.put(entry, "display_name", actor.display_name))
  end

  defp same_message?(game, actor),
    do:
      game.guild_id == actor.guild_id and game.channel_id == actor.channel_id and
        game.message_id == actor.message_id

  defp host_or_admin?(game, actor), do: actor.discord_id == game.host_discord_id or actor.admin?

  defp authorize(actor) do
    guild = Application.get_env(:the_gathering, TheGathering.Discord, [])[:guild_id]

    if actor.guild_id not in [nil, ""] and actor.discord_id not in [nil, ""] and
         (guild in [nil, ""] or to_string(guild) == actor.guild_id),
       do: :ok,
       else: {:error, :forbidden}
  end
end
