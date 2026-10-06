defmodule TheGathering.Discord.NewGameMessage do
  @moduledoc false
  alias TheGathering.Discord.ScheduledGames
  alias TheGatheringWeb.Endpoint

  def render(game) do
    roster = roster(game.players)

    %{
      content: "",
      allowed_mentions: :none,
      embeds: [
        %{
          title: game.title,
          description: description(game),
          fields:
            [
              %{name: "Start", value: start(game.start_at)},
              %{name: "Minimum", value: to_string(game.min_players), inline: true},
              %{name: "Format", value: game.format || "Commander", inline: true},
              %{
                name: "Players (#{map_size(game.players)}/10)",
                value: if(roster == "", do: "No players yet. Click Join!", else: roster)
              }
            ] ++ maybe_field(game)
        }
      ],
      components: [
        %{
          type: 1,
          components: [
            button(game, "join", "Join", 3),
            button(game, "maybe", "Maybe", 2),
            button(game, "leave", "Leave", 2),
            button(game, "time", "Change time", 1),
            button(game, "cancel", "Cancel", 4)
          ]
        }
      ]
    }
  end

  def announcement(game) do
    ids = Map.keys(game.players) |> Enum.sort()

    %{
      content:
        Enum.map_join(ids, " ", &"<@#{&1}>") <>
          " Your game is ready! " <> url(game) <> "\nSign in with Discord to join the table.",
      allowed_mentions: [users: ids],
      nonce: "newgame:#{game.id}",
      enforce_nonce: true
    }
  end

  def maybe_ping(game) do
    ids = Map.keys(game.maybe) |> Enum.sort()
    missing = game.min_players - map_size(game.players)
    deadline = deadline(game)

    %{
      content:
        Enum.map_join(ids, " ", &"<@#{&1}>") <>
          " **#{game.title}** is #{missing} #{if missing == 1, do: "player", else: "players"} short" <>
          " at its start time. Click **Join** on the game by <t:#{deadline}:t> (<t:#{deadline}:R>)" <>
          " if you can play: https://discord.com/channels/#{game.guild_id}/#{game.channel_id}/#{game.message_id}",
      allowed_mentions: [users: ids],
      # Discord nonces are capped at 25 characters; the ping time distinguishes re-pings.
      nonce: "ngm:#{game.id}:#{DateTime.to_unix(game.maybe_pinged_at)}",
      enforce_nonce: true
    }
  end

  defp roster(list) do
    list
    |> Enum.sort_by(fn {id, entry} -> {entry["joined_at"], id} end)
    |> Enum.map_join("\n", fn {id, _entry} -> "<@#{id}>" end)
  end

  defp maybe_field(%{maybe: maybe}) when map_size(maybe) == 0, do: []

  defp maybe_field(game),
    do: [%{name: "Maybe (#{map_size(game.maybe)}) — not counted", value: roster(game.maybe)}]

  defp deadline(game),
    do: DateTime.to_unix(game.maybe_pinged_at) + ScheduledGames.maybe_grace_seconds()

  defp description(%{status: "started"} = game),
    do: "Your game is ready! [Open lobby](#{url(game)})"

  defp description(%{status: "expired"}), do: "This game did not fill before its start time."
  defp description(%{status: "cancelled"}), do: "This game was cancelled."

  defp description(%{maybe_pinged_at: %DateTime{}} = game),
    do:
      "Short of the minimum at start time, so the maybe list was pinged. " <>
        "Join by <t:#{deadline(game)}:t> (<t:#{deadline(game)}:R>) or this game expires."

  defp description(%{start_at: nil}),
    do:
      "Join the roster to play. Maybe doesn't count toward the minimum. " <>
        "The host or a Discord Administrator can change the time or cancel."

  defp description(_game),
    do:
      "Join the roster to play. Maybe doesn't count toward the minimum, but if the game is " <>
        "short at its start time, the maybe list is pinged. " <>
        "The host or a Discord Administrator can change the time or cancel."

  defp start(nil), do: "As soon as the minimum is met"
  defp start(time), do: "<t:#{DateTime.to_unix(time)}:F> (<t:#{DateTime.to_unix(time)}:R>)"
  defp url(game), do: Endpoint.url() <> "/table/" <> game.room_id

  defp button(game, action, label, style),
    do: %{
      type: 2,
      custom_id: "newgame:#{game.id}:#{action}",
      label: label,
      style: style,
      disabled: game.status != "open"
    }
end
