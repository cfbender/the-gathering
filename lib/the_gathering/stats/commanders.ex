defmodule TheGathering.Stats.Commanders do
  @moduledoc """
  Commander statistics aggregated across every player and deck.

  A seat counts once for each commander card its deck ran, so a partner deck
  contributes to both partners, and a mirror match contributes every matching seat.

  Every stored commander reference (Scryfall ID and/or card name) is resolved to one
  canonical identity before anything is grouped: the catalog card it names, when the
  catalog knows it, otherwise the stored ID, otherwise the normalized name. The
  published `id` is that canonical catalog ID (or the card name when the catalog lacks
  the card), and `get/2` accepts the published ID, a stored ID, or a card name, so every
  ID `list/1` emits resolves to a detail page.
  """

  import Ecto.Query
  import TheGathering.Stats.Records

  alias TheGathering.Catalog
  alias TheGathering.Catalog.CardData
  alias TheGathering.Games.GamePlayer
  alias TheGathering.Repo

  @doc "Every commander played, most played first."
  def list(params \\ %{}) do
    params |> seats() |> summarize()
  end

  @doc "`list/1` over seats that already have `:game`, `:deck`, and `:player` loaded."
  def summarize(seats) do
    summaries = card_summaries(seats)

    seats
    |> Enum.flat_map(&commander_seats(&1, summaries))
    |> commander_records(fn seats ->
      %{
        pilots: seats |> Enum.map(& &1.player_id) |> Enum.uniq() |> length(),
        decks: seats |> Enum.map(& &1.deck_id) |> Enum.uniq() |> length(),
        last_played_at: seats |> Enum.map(& &1.game.played_at) |> Enum.max(DateTime)
      }
    end)
  end

  @doc """
  Detail for one commander addressed by its published ID, a stored Scryfall ID, or a
  card name; `nil` when never played.
  """
  def get(id, params \\ %{}) do
    all_seats = seats(params)
    summaries = card_summaries(all_seats)

    entries =
      all_seats
      |> Enum.flat_map(&commander_seats(&1, summaries))
      |> Enum.filter(&matches?(&1, id))

    case entries do
      [] ->
        nil

      [{key, card, _seat} | _] ->
        seats = Enum.map(entries, fn {_key, _card, seat} -> seat end)
        seat_ids = MapSet.new(seats, & &1.id)
        games = seats |> Enum.map(& &1.game) |> Enum.uniq_by(& &1.id) |> preload_seats()
        tracked = &Enum.filter(&1, fn seat -> MapSet.member?(seat_ids, seat.id) end)

        %{
          commander: commander_entity(key, card),
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

  # One `{key, card, seat}` entry per commander card the seat's deck ran, where `key`
  # is the canonical identity and `card` carries the resolved name plus the stored
  # reference so `matches?/2` can honour legacy IDs.
  defp commander_seats(seat, summaries) do
    deck = seat.deck

    [
      {deck.commander_card_id, deck.commander_name},
      {deck.partner_card_id, deck.partner_name}
    ]
    |> Enum.reject(fn {id, name} -> is_nil(id) and (is_nil(name) or name == "") end)
    |> Enum.map(fn {id, name} ->
      {key, card} = canonical(Catalog.card_summary(summaries, id, name), id, name)
      {key, card, seat}
    end)
  end

  defp canonical(%{id: catalog_id} = summary, stored_id, _name) do
    {{:id, catalog_id}, Map.put(summary, :stored_id, stored_id)}
  end

  defp canonical(nil, stored_id, name) when is_binary(stored_id) do
    {{:id, stored_id}, missing_card(stored_id, name)}
  end

  defp canonical(nil, nil, name) do
    {{:name, CardData.normalize_name(name)}, missing_card(nil, name)}
  end

  defp missing_card(stored_id, name),
    do: %{id: stored_id, name: name, art_crop_url: nil, color_identity: nil, stored_id: stored_id}

  # `id` is a published/canonical ID, a stored Scryfall ID, or a card name.
  defp matches?({key, card, _seat}, id) do
    key == {:id, id} or card.stored_id == id or
      (is_binary(card.name) and CardData.normalize_name(card.name) == CardData.normalize_name(id))
  end

  defp commander_entity(key, card) do
    %{
      id: public_id(key, card),
      name: card.name,
      art_crop_url: card.art_crop_url,
      color_identity: card.color_identity
    }
  end

  # Cards the catalog lacks and that were only ever recorded by name are addressed by
  # that name; everything else by its canonical Scryfall ID.
  defp public_id({:id, id}, _card), do: id
  defp public_id({:name, _normalized}, card), do: card.name

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
    |> Enum.flat_map(&commander_seats(&1, summaries))
    |> Enum.reject(fn {other_key, _card, _seat} -> other_key == key end)
    |> commander_records()
  end

  # Groups `{key, card, seat}` entries per commander into entity + record (+ `extra_fun.(seats)`).
  defp commander_records(entries, extra_fun \\ fn _seats -> %{} end) do
    entries
    |> Enum.group_by(fn {key, _card, _seat} -> key end)
    |> Enum.map(fn {key, rows} ->
      {_key, card, _seat} = hd(rows)
      seats = Enum.map(rows, fn {_key, _card, seat} -> seat end)

      key
      |> commander_entity(card)
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
