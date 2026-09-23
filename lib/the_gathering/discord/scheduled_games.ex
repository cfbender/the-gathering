defmodule TheGathering.Discord.ScheduledGames do
  @moduledoc "Durable queue transitions. SQLite immediate transactions serialize roster changes."
  import Ecto.Query
  alias TheGathering.Discord.ScheduledGame
  alias TheGathering.Repo

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

  def act(id, action, actor, now) do
    Repo.transaction(fn ->
      with :ok <- authorize(actor),
           %ScheduledGame{} = game <- Repo.get(ScheduledGame, id),
           true <-
             game.guild_id == actor.guild_id and game.channel_id == actor.channel_id and
               game.message_id == actor.message_id,
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
      due? -> transition(game, if(enough?, do: "started", else: "expired"), now)
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

  defp changes(game, "join", actor, now) do
    if map_size(game.players) >= 10 and not Map.has_key?(game.players, actor.discord_id) do
      {:error, :full}
    else
      player = Map.get(game.players, actor.discord_id, %{"joined_at" => DateTime.to_iso8601(now)})
      player = Map.put(player, "display_name", actor.display_name)
      {:ok, [players: Map.put(game.players, actor.discord_id, player), message_dirty: true]}
    end
  end

  defp changes(game, "leave", actor, _now),
    do: {:ok, [players: Map.delete(game.players, actor.discord_id), message_dirty: true]}

  defp changes(game, "cancel", actor, _now) do
    if actor.discord_id == game.host_discord_id or actor.admin?,
      do: {:ok, [status: "cancelled", message_dirty: true]},
      else: {:error, :forbidden}
  end

  defp changes(_game, _action, _actor, _now), do: {:error, :forbidden}

  defp authorize(actor) do
    guild = Application.get_env(:the_gathering, TheGathering.Discord, [])[:guild_id]

    if actor.guild_id not in [nil, ""] and actor.discord_id not in [nil, ""] and
         (guild in [nil, ""] or to_string(guild) == actor.guild_id),
       do: :ok,
       else: {:error, :forbidden}
  end
end
