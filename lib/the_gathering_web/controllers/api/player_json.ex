defmodule TheGatheringWeb.API.PlayerJSON do
  alias TheGathering.Games.Player
  alias TheGatheringWeb.API.{DeckJSON, GameJSON}

  def index(%{players: players}), do: %{data: Enum.map(players, &summary/1)}
  def show(%{player: player, card_art: card_art}), do: %{data: detail(player, card_art)}

  def summary(%Player{} = player),
    do: %{
      id: player.id,
      name: player.name,
      avatar_url: player.avatar_url,
      user_id: player.user_id,
      archived_at: player.archived_at
    }

  def card_refs(%Player{} = player) do
    DeckJSON.card_refs(player.decks) ++
      Enum.flat_map(player.game_players, fn seat ->
        if seat.deck, do: DeckJSON.card_refs(seat.deck), else: []
      end)
  end

  defp detail(player, card_art) do
    seats = player.game_players

    summary(player)
    |> Map.merge(%{
      discord_id: player.discord_id,
      games_played: length(seats),
      wins: Enum.count(seats, &(&1.result == "win")),
      decks: Enum.map(player.decks, &DeckJSON.summary(&1, card_art)),
      recent_games: seats |> Enum.take(10) |> Enum.map(&GameJSON.seat_game(&1, card_art))
    })
  end
end
