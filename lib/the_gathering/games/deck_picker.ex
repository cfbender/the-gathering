defmodule TheGathering.Games.DeckPicker do
  @moduledoc false

  import Ecto.Query

  alias TheGathering.Accounts.User
  alias TheGathering.Games.{Deck, Game, GamePlayer, Player}
  alias TheGathering.Repo

  @unplayed_boost_hours 30 * 24

  def random_deck(%User{} = user, opts \\ []) do
    case Repo.get_by(Player, user_id: user.id) do
      nil ->
        {:ok, %{deck: nil, reason: :player_not_linked}}

      player ->
        candidates = playable_decks(player.id)
        candidates = maybe_exclude_deck(candidates, Keyword.get(opts, :exclude_id))
        now = Keyword.get(opts, :now, DateTime.utc_now())
        random = Keyword.get(opts, :random, &:rand.uniform/0)

        {:ok,
         case weighted_pick(selection_weights(candidates, now), random.()) do
           nil -> %{deck: nil, reason: :no_eligible_decks}
           candidate -> Map.put(candidate, :reason, nil)
         end}
    end
  end

  def selection_weights(candidates, now) do
    played_recencies =
      for %{last_played_at: %DateTime{} = played_at} <- candidates do
        recency_hours(now, played_at)
      end

    unplayed_recency =
      max(Enum.max(played_recencies, fn -> 0 end) + @unplayed_boost_hours, @unplayed_boost_hours)

    Enum.map(candidates, fn candidate ->
      recency =
        case candidate.last_played_at do
          %DateTime{} = played_at -> recency_hours(now, played_at)
          nil -> unplayed_recency
        end

      weight = recency * (candidate.deck.skip_count + 1) / (candidate.play_count + 1)
      Map.put(candidate, :weight, weight)
    end)
  end

  def record_outcome(%User{} = user, deck_id, outcome) when outcome in [:played, :skipped] do
    with %Player{} = player <- Repo.get_by(Player, user_id: user.id),
         %Deck{} = deck <- Repo.get_by(Deck, id: deck_id, player_id: player.id),
         false <- not is_nil(deck.archived_at) do
      updates = if outcome == :played, do: [set: [skip_count: 0]], else: [inc: [skip_count: 1]]
      {1, _rows} = Repo.update_all(from(item in Deck, where: item.id == ^deck.id), updates)
      {:ok, Repo.get!(Deck, deck.id)}
    else
      nil -> {:error, :not_found}
      true -> {:error, :bad_request}
    end
  end

  def record_outcome(%User{}, _deck_id, _outcome), do: {:error, :bad_request}

  defp playable_decks(player_id) do
    from(deck in Deck,
      left_join: seat in GamePlayer,
      on: seat.deck_id == deck.id,
      left_join: game in Game,
      on: game.id == seat.game_id,
      where: deck.player_id == ^player_id and is_nil(deck.archived_at) and deck.included_for_play,
      group_by: deck.id,
      order_by: [asc: fragment("lower(?)", deck.name), asc: deck.id],
      select: %{
        deck: deck,
        play_count: count(seat.id),
        last_played_at: max(game.played_at)
      }
    )
    |> Repo.all()
  end

  defp maybe_exclude_deck(candidates, exclude_id) when length(candidates) > 1 do
    case Ecto.Type.cast(:id, exclude_id) do
      {:ok, id} -> Enum.reject(candidates, &(&1.deck.id == id))
      :error -> candidates
    end
  end

  defp maybe_exclude_deck(candidates, _exclude_id), do: candidates

  defp recency_hours(now, played_at), do: max(DateTime.diff(now, played_at, :hour) + 1, 1)

  defp weighted_pick([], _random), do: nil

  defp weighted_pick(weighted, random) do
    total = Enum.reduce(weighted, 0.0, &(&1.weight + &2))
    threshold = min(max(random, 0.0), 1.0) * total
    last = List.last(weighted)

    weighted
    |> Enum.reduce_while(0.0, fn candidate, cumulative ->
      cumulative = cumulative + candidate.weight
      if cumulative >= threshold, do: {:halt, candidate}, else: {:cont, cumulative}
    end)
    |> case do
      candidate when is_map(candidate) -> candidate
      _cumulative -> last
    end
  end
end
