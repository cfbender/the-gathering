defmodule TheGathering.Games.MergePlayers do
  @moduledoc false

  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Accounts.User
  alias TheGathering.Games.{Deck, GamePlayer, Player}
  alias TheGathering.Repo

  def run(%Player{id: id}, %Player{id: id}), do: {:error, :bad_request}

  def run(%Player{} = source, %Player{} = target) do
    Repo.transaction(fn ->
      with :ok <- ensure_mergeable(source, target),
           {:ok, target} <- carry_identity(source, target) do
        move_decks(source, target)
        migrate_player_references(source, target)
        Repo.delete!(source)
        Repo.get!(Player, target.id)
      else
        {:error, reason} -> Repo.rollback(reason)
      end
    end)
  end

  def link_to_user(%Player{} = player, %User{} = user) do
    player = Repo.get!(Player, player.id)
    current = Repo.get_by(Player, user_id: user.id)

    cond do
      current && current.id == player.id ->
        {:ok, player}

      conflicting?(player.user_id, user.id) ->
        {:error, merge_error(player, "players belong to different accounts")}

      conflicting?(player.discord_id, user.discord_id) ->
        {:error, merge_error(player, "players have different Discord identities")}

      current ->
        run(current, player)

      true ->
        player
        |> change(user_id: user.id)
        |> maybe_put_discord_id(user.discord_id)
        |> Repo.update()
    end
  end

  defp ensure_mergeable(source, target) do
    shared_games =
      Repo.exists?(
        from a in GamePlayer,
          join: b in GamePlayer,
          on: a.game_id == b.game_id,
          where: a.player_id == ^source.id and b.player_id == ^target.id
      )

    cond do
      shared_games ->
        {:error, merge_error(source, "both players are seated in the same game")}

      conflicting?(source.user_id, target.user_id) ->
        {:error, merge_error(source, "players belong to different accounts")}

      conflicting?(source.discord_id, target.discord_id) ->
        {:error, merge_error(source, "players have different Discord identities")}

      true ->
        :ok
    end
  end

  defp conflicting?(a, b), do: not is_nil(a) and not is_nil(b) and a != b

  defp merge_error(source, message), do: source |> change() |> add_error(:merge, message)

  defp carry_identity(source, target) do
    source |> change(user_id: nil, discord_id: nil) |> Repo.update!()

    target
    |> change(user_id: target.user_id || source.user_id)
    |> maybe_put_discord_id(source.discord_id)
    |> Repo.update()
  end

  defp maybe_put_discord_id(changeset, nil), do: changeset

  defp maybe_put_discord_id(changeset, discord_id) do
    case get_field(changeset, :discord_id) do
      nil -> put_change(changeset, :discord_id, discord_id)
      _existing -> changeset
    end
  end

  defp move_decks(source, target) do
    target_decks =
      Repo.all(from deck in Deck, where: deck.player_id == ^target.id)
      |> Map.new(&{fold_name(&1.name), &1})

    for deck <- Repo.all(from deck in Deck, where: deck.player_id == ^source.id) do
      case Map.fetch(target_decks, fold_name(deck.name)) do
        {:ok, existing} ->
          Repo.update_all(from(seat in GamePlayer, where: seat.deck_id == ^deck.id),
            set: [deck_id: existing.id]
          )

          Repo.delete!(deck)

        :error ->
          deck |> change(player_id: target.id) |> Repo.update!()
      end
    end

    :ok
  end

  defp migrate_player_references(source, target) do
    Repo.update_all(from(seat in GamePlayer, where: seat.player_id == ^source.id),
      set: [player_id: target.id]
    )

    Repo.update_all(from(seat in GamePlayer, where: seat.eliminated_by_player_id == ^source.id),
      set: [eliminated_by_player_id: target.id]
    )
  end

  defp fold_name(name), do: name |> String.trim() |> String.downcase(:ascii)
end
