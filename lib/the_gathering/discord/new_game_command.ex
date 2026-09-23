defmodule TheGathering.Discord.NewGameCommand do
  @moduledoc false
  require Logger
  alias Nostrum.Struct.Guild.Member
  alias TheGathering.Discord.{NewGameScheduler, ScheduledGames, StartTime}

  def definition do
    %{
      name: "newgame",
      description: "Gather players for a webcam table",
      dm_permission: false,
      options: [
        %{
          type: 3,
          name: "start",
          description:
            "8pm, 20:30, in 45m, tomorrow 7pm, or <t:unix>; omitted starts when filled",
          required: false,
          max_length: 100
        },
        %{
          type: 4,
          name: "min_players",
          description: "Minimum players (default 3)",
          required: false,
          min_value: 2,
          max_value: 10
        },
        %{type: 3, name: "title", description: "Game title", required: false, max_length: 100},
        %{
          type: 3,
          name: "format",
          description: "Game format (default Commander)",
          required: false,
          max_length: 100
        }
      ]
    }
  end

  def respond(
        interaction,
        api \\ Nostrum.Api.Interaction,
        scheduler \\ NewGameScheduler,
        now \\ DateTime.utc_now(),
        guild_cache \\ Nostrum.Cache.GuildCache
      ) do
    if Map.get(interaction.data, :custom_id) do
      button(interaction, api, scheduler, guild_cache)
    else
      create(interaction, api, scheduler, now)
    end
  end

  defp create(interaction, api, scheduler, now) do
    options = Map.new(Map.get(interaction.data, :options) || [], &{&1.name, &1.value})

    with {:ok, start_at} <- StartTime.parse(options["start"], now),
         {:ok, game} <-
           ScheduledGames.create(Map.put(options, "start_at", start_at), actor(interaction)) do
      # Persist the placeholder's ID before exposing buttons. Once attached, even
      # a failed first queue edit is durable work the scheduler retries after boot.
      with {:ok} <- api.create_response(interaction, %{type: 5}),
           {:ok, message} <-
             api.edit_response(interaction, %{
               content: "Preparing your game…",
               allowed_mentions: %{parse: []}
             }) do
        NewGameScheduler.attach(game.id, message.id, scheduler)
      else
        {:error, _} ->
          ScheduledGames.cancel_unpublished(game.id)
          Logger.warning("Discord newgame #{game.id} initial response failed")
          {:error, :delivery_failed}
      end
    else
      {:error, reason} ->
        api.create_response(interaction, %{type: 4, data: private(error(reason))})
    end
  end

  defp button(interaction, api, scheduler, guild_cache) do
    with ["newgame", id, action] <- String.split(interaction.data.custom_id, ":"),
         {id, ""} when id > 0 <- Integer.parse(id),
         true <- action in ["join", "leave", "cancel"] do
      with {:ok} <- api.create_response(interaction, %{type: 5, data: %{flags: 64}}) do
        actor =
          Map.put(
            actor(interaction),
            :admin?,
            action == "cancel" and administrator?(interaction, guild_cache)
          )

        result = NewGameScheduler.act(id, action, actor, scheduler)
        api.edit_response(interaction, private(confirmation(result, action)))
      end
    else
      _ ->
        api.create_response(interaction, %{type: 4, data: private("This game button is invalid.")})
    end
  end

  defp actor(interaction) do
    member = Map.get(interaction, :member) || %{}
    user = Map.get(interaction, :user) || Map.get(member, :user) || %{}
    message = Map.get(interaction, :message) || %{}

    %{
      discord_id: to_string(Map.get(user, :id) || Map.get(member, :user_id)),
      guild_id: to_string(Map.get(interaction, :guild_id)),
      channel_id: to_string(Map.get(interaction, :channel_id)),
      message_id: to_string(Map.get(message, :id)),
      display_name:
        Map.get(member, :nick) || Map.get(user, :global_name) || Map.get(user, :username) ||
          "Player",
      admin?: false
    }
  end

  # Nostrum 0.10 drops member.permissions from interactions. Use the guild's cached
  # roles and the interaction's member roles instead; no Guild Members intent needed.
  defp administrator?(
         %{guild_id: guild_id, member: %Member{} = member},
         cache
       )
       when not is_nil(guild_id) do
    case cache.get(guild_id) do
      {:ok, guild} ->
        :administrator in Member.guild_permissions(member, guild)

      _ ->
        false
    end
  end

  defp administrator?(_interaction, _cache), do: false

  defp confirmation({:ok, %{status: "started"}}, _),
    do: "Your game is ready! See the lobby link in the channel."

  defp confirmation({:ok, %{status: "expired"}}, _),
    do: "This game did not fill before its start time."

  defp confirmation({:ok, %{status: "cancelled"}}, _), do: "This game was cancelled."
  defp confirmation({:ok, _}, "join"), do: "You are on the roster."
  defp confirmation({:ok, _}, "leave"), do: "You are no longer on the roster."
  defp confirmation({:error, reason}, _), do: error(reason)
  defp error(message) when is_binary(message), do: message
  defp error(:full), do: "This game already has 10 players."

  defp error(:forbidden),
    do:
      "Use this game's original server and channel. Only its host or a Discord Administrator can cancel."

  defp error(_), do: "Use a minimum of 2–10 players and a title/format of at most 100 characters."
  defp private(content), do: %{content: content, flags: 64, allowed_mentions: %{parse: []}}
end
