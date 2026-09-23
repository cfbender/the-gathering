defmodule TheGathering.Discord.NewGameMessage do
  @moduledoc false
  alias TheGatheringWeb.Endpoint

  def render(game) do
    roster =
      game.players
      |> Enum.sort_by(fn {id, player} -> {player["joined_at"], id} end)
      |> Enum.map_join("\n", fn {id, _player} -> "<@#{id}>" end)

    %{
      content: "",
      allowed_mentions: %{parse: []},
      embeds: [
        %{
          title: game.title,
          description: description(game),
          fields: [
            %{name: "Start", value: start(game.start_at)},
            %{name: "Minimum", value: to_string(game.min_players), inline: true},
            %{name: "Format", value: game.format || "Commander", inline: true},
            %{
              name: "Players (#{map_size(game.players)}/10)",
              value: if(roster == "", do: "No players yet. Click Join!", else: roster)
            }
          ]
        }
      ],
      components: [
        %{
          type: 1,
          components: [
            button(game, "join", "Join", 3),
            button(game, "leave", "Leave", 2),
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
      allowed_mentions: %{parse: [], users: ids},
      nonce: "newgame:#{game.id}",
      enforce_nonce: true
    }
  end

  defp description(%{status: "started"} = game),
    do: "Your game is ready! [Open lobby](#{url(game)})"

  defp description(%{status: "expired"}), do: "This game did not fill before its start time."
  defp description(%{status: "cancelled"}), do: "This game was cancelled."

  defp description(_game),
    do: "Join the roster to play. The host or a Discord Administrator can cancel."

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
