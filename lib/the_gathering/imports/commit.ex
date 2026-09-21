defmodule TheGathering.Imports.Commit do
  @moduledoc false

  alias TheGathering.Games
  alias TheGathering.Games.{Game, LinkCatalogCards}
  alias TheGathering.Imports.Preview
  alias TheGathering.Repo

  @deck_attrs [
    :commander_card_id,
    :partner_name,
    :partner_card_id,
    :color_identity,
    :decklist_url
  ]

  def run(source, payload, source_name, user_id) do
    preview = Preview.run(source, payload)

    if preview.valid do
      commit(preview.games, source_name, user_id)
    else
      {:error, {:validation, preview}}
    end
  end

  defp commit(games, source, user_id) do
    case Repo.transaction(fn -> commit_games(games, source, user_id) end) do
      {:ok, result} ->
        Enum.each(result.game_ids, &LinkCatalogCards.link_game/1)
        {:ok, result}

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp commit_games(games, source, user_id) do
    games
    |> Enum.reduce(
      %{created: 0, skipped: 0, game_ids: []},
      &commit_game(&1, &2, source, user_id)
    )
    |> Map.update!(:game_ids, &Enum.reverse/1)
  end

  defp commit_game(game, result, source, user_id) do
    case Repo.get_by(Game, source: source, external_id: game.external_id) do
      %Game{id: id} ->
        %{result | skipped: result.skipped + 1, game_ids: [id | result.game_ids]}

      nil ->
        seats = Enum.map(game.seats, &commit_seat/1)

        attrs = %{
          played_at: game.played_at,
          duration_minutes: game.duration_minutes,
          turns: game.turns,
          win_condition: game.win_condition,
          notes: game.notes,
          source: source,
          external_id: game.external_id,
          seats: seats
        }

        case Games.create_game(attrs, user_id) do
          {:ok, created} ->
            %{result | created: result.created + 1, game_ids: [created.id | result.game_ids]}

          {:error, changeset} ->
            Repo.rollback({:changeset, changeset})
        end
    end
  end

  def commit_seat(seat) do
    player = seat |> resolve_player() |> unwrap!()

    deck_attrs =
      seat
      |> Map.take(@deck_attrs)
      |> Map.reject(fn {_key, value} -> value in [nil, ""] end)
      |> Map.put(:commander_name, seat.commander)

    deck = player |> Games.find_or_create_deck(seat.deck, deck_attrs) |> unwrap!()

    %{
      player_id: player.id,
      deck_id: deck.id,
      seat: seat.seat,
      result: seat.result,
      kills: seat.kills,
      mvp_card_name: seat.mvp_card,
      mvp_card_id: seat.mvp_card_id
    }
  end

  defp resolve_player(seat), do: Games.resolve_player(seat.player, seat.discord_id)

  defp unwrap!({:ok, value}), do: value
  defp unwrap!({:error, changeset}), do: Repo.rollback({:changeset, changeset})
end
