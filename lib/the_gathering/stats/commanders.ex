defmodule TheGathering.Stats.Commanders do
  @moduledoc """
  Commander statistics aggregated across every player and deck.

  A seat counts once for each commander card its deck ran, so a partner deck
  contributes to both partners. Commanders are keyed by Scryfall card ID; decks
  that only recorded a card name (older imports) fall back to the normalized name.
  """

  import Ecto.Query
  import TheGathering.Stats.Records

  alias TheGathering.Catalog
  alias TheGathering.Catalog.CardData
  alias TheGathering.Games.GamePlayer
  alias TheGathering.Repo

  @doc "Every commander played, most played first."
  def list(params \\ %{}) do
    seats = seats(params)
    summaries = card_summaries(seats)

    seats
    |> Enum.flat_map(&commander_seats/1)
    |> commander_records(summaries, fn seats ->
      %{
        pilots: seats |> Enum.map(& &1.player_id) |> Enum.uniq() |> length(),
        decks: seats |> Enum.map(& &1.deck_id) |> Enum.uniq() |> length(),
        last_played_at: seats |> Enum.map(& &1.game.played_at) |> Enum.max(DateTime)
      }
    end)
  end

  @doc "Detail for one commander by Scryfall ID or card name; `nil` when never played."
  def get(id, params \\ %{}) do
    seats = params |> seats() |> Enum.filter(&commander_seat?(&1, id))

    case seats do
      [] ->
        nil

      [first | _] ->
        {key, card, _seat} = first |> commander_seats() |> Enum.find(&matches?(&1, id))
        summaries = card_summaries(seats)
        seat_ids = MapSet.new(seats, & &1.id)
        games = seats |> Enum.map(& &1.game) |> Enum.uniq_by(& &1.id) |> preload_seats()
        tracked = &Enum.find(&1, fn seat -> MapSet.member?(seat_ids, seat.id) end)

        %{
          commander: commander_entity(card, key, summaries),
          record: record(seats),
          pilots: grouped_records(seats, & &1.player, & &1.player_id),
          decks: grouped_records(seats, &deck_entity(&1.deck, summaries), & &1.deck_id),
          partners: partners(seats, key, summaries),
          opponents: opponents(games, seat_ids),
          win_rate_over_time: cumulative_win_rate(games, tracked),
          recent_games: games |> Enum.take(10) |> Enum.map(&recent_game(&1, tracked.(&1.seats)))
        }
    end
  end

  defp seats(params) do
    GamePlayer
    |> join(:inner, [seat], game in assoc(seat, :game), as: :game)
    |> join(:inner, [seat], deck in assoc(seat, :deck))
    |> date_range(params)
    |> order_by([seat, game: game], desc: game.played_at, desc: game.id, asc: seat.seat)
    |> preload([:player, :deck, :game])
    |> Repo.all()
  end

  defp preload_seats(games) do
    games
    |> Repo.preload(seats: [:player, :deck])
    |> Enum.sort_by(&{DateTime.to_unix(&1.played_at), &1.id}, :desc)
  end

  # One `{key, card, seat}` entry per commander card the seat's deck ran.
  defp commander_seats(seat) do
    deck = seat.deck

    [
      {deck.commander_card_id, deck.commander_name},
      {deck.partner_card_id, deck.partner_name}
    ]
    |> Enum.reject(fn {id, name} -> is_nil(id) and (is_nil(name) or name == "") end)
    |> Enum.map(fn {id, name} -> {key(id, name), %{id: id, name: name}, seat} end)
  end

  defp key(id, _name) when is_binary(id), do: {:id, id}
  defp key(nil, name), do: {:name, CardData.normalize_name(name)}

  defp commander_seat?(seat, id), do: seat |> commander_seats() |> Enum.any?(&matches?(&1, id))

  # `id` is either a Scryfall card ID or a card name (for cards missing from the catalog).
  defp matches?({key, card, _seat}, id) do
    key == {:id, id} or
      (is_binary(card.name) and CardData.normalize_name(card.name) == CardData.normalize_name(id))
  end

  defp commander_entity(card, key, summaries) do
    summary = Catalog.card_summary(summaries, card.id, card.name)

    %{
      id: public_id(key, summary, card),
      name: (summary && summary.name) || card.name,
      art_crop_url: summary && summary.art_crop_url,
      color_identity: (summary && summary.color_identity) || nil
    }
  end

  # Prefer a catalog ID even for name-only decks once the card is known, so links stay
  # stable; cards missing from the catalog are addressed by name (`get/2` accepts either).
  defp public_id({:id, id}, _summary, _card), do: id
  defp public_id({:name, _normalized}, %{id: id}, _card), do: id
  defp public_id({:name, _normalized}, nil, card), do: card.name

  defp deck_entity(deck, summaries) do
    art = Catalog.card_summary(summaries, deck.commander_card_id, deck.commander_name)

    %{
      id: deck.id,
      name: deck.name,
      commander_name: deck.commander_name,
      color_identity: deck.color_identity,
      art_crop_url: art && art.art_crop_url
    }
  end

  defp partners(seats, key, summaries) do
    seats
    |> Enum.flat_map(&commander_seats/1)
    |> Enum.reject(fn {other_key, _card, _seat} -> other_key == key end)
    |> commander_records(summaries)
  end

  # Groups `{key, card, seat}` entries per commander into entity + record (+ `extra_fun.(seats)`).
  defp commander_records(entries, summaries, extra_fun \\ fn _seats -> %{} end) do
    entries
    |> Enum.group_by(fn {key, _card, _seat} -> key end)
    |> Enum.map(fn {key, rows} ->
      {_key, card, _seat} = hd(rows)
      seats = Enum.map(rows, fn {_key, _card, seat} -> seat end)

      card
      |> commander_entity(key, summaries)
      |> Map.merge(record(seats))
      |> Map.merge(extra_fun.(seats))
    end)
    |> Enum.sort_by(&{-&1.games, -&1.win_rate, String.downcase(&1.name)})
  end

  defp opponents(games, seat_ids) do
    games
    |> Enum.flat_map(fn game -> Enum.reject(game.seats, &MapSet.member?(seat_ids, &1.id)) end)
    |> Enum.reject(&is_nil(&1.player))
    |> grouped_records(& &1.player, & &1.player_id)
  end

  defp card_summaries(seats) do
    Catalog.card_summaries(
      Enum.flat_map(seats, fn seat ->
        [
          {seat.deck.commander_card_id, seat.deck.commander_name},
          {seat.deck.partner_card_id, seat.deck.partner_name}
        ]
      end)
    )
  end
end
