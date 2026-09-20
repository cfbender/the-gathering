defmodule TheGatheringWeb.API.PlayerJSON do
  alias TheGathering.Games.Player
  alias TheGatheringWeb.API.{DeckJSON, GameJSON}

  def index(%{players: players}), do: %{data: Enum.map(players, &summary/1)}
  def show(%{player: player}), do: %{data: detail(player)}

  def summary(%Player{} = player),
    do: %{
      id: player.id,
      name: player.name,
      avatar_url: player.avatar_url,
      user_id: player.user_id,
      archived_at: player.archived_at
    }

  defp detail(player) do
    seats = player.game_players

    summary(player)
    |> Map.merge(%{
      discord_id: player.discord_id,
      games_played: length(seats),
      wins: Enum.count(seats, &(&1.result == "win")),
      decks: Enum.map(player.decks, &DeckJSON.summary/1),
      recent_games: seats |> Enum.take(10) |> Enum.map(&GameJSON.seat_game/1)
    })
  end
end
