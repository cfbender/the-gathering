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
  ID `list/1` emits resolves to a detail page. Detail trends are limited to the newest
  500 games so response and calculation cost remain bounded.
  """

  alias TheGathering.Catalog
  alias TheGathering.Catalog.CardData
  alias TheGathering.Stats.{Query, Records, Summaries}

  @trend_game_limit 500

  @doc "Every commander played, most played first."
  def list(params \\ %{}) do
    params |> Query.commander_seats() |> summarize()
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
    with {key, card} <- resolve(id) do
      ids = [id, card.id, card.stored_id] |> Enum.reject(&is_nil/1) |> Enum.uniq()
      names = if is_binary(card.name), do: [card.name], else: []
      {stored_ids, stored_names} = Query.commander_aliases(ids, names)
      ids = Enum.uniq(ids ++ stored_ids)
      names = Enum.uniq(names ++ stored_names)
      candidate_seats = Query.commander_seats(params, ids, names)
      summaries = card_summaries(candidate_seats)

      entries =
        candidate_seats
        |> Enum.flat_map(&commander_seats(&1, summaries))
        |> Enum.filter(&same_commander?(&1, key))

      detail(entries, summaries)
    end
  end

  defp detail([], _summaries), do: nil

  defp detail([{key, card, _seat} | _] = entries, summaries) do
    seats = Enum.map(entries, fn {_key, _card, seat} -> seat end)
    seat_ids = Enum.map(seats, & &1.id)
    game_ids = seats |> Enum.map(& &1.game_id) |> Enum.uniq()
    recent_games = Query.recent_games(game_ids, 10)
    tracked_seat_ids = MapSet.new(seat_ids)

    %{
      commander: Summaries.commander(key, card),
      record: Records.record(seats),
      pilots: Records.grouped_records(seats, &Summaries.entity(&1.player), & &1.player_id),
      decks:
        Records.grouped_records(
          seats,
          &Summaries.deck(&1.deck, summaries),
          & &1.deck_id
        ),
      partners: partners(seats, key, summaries),
      opponents: game_ids |> Query.opponent_counts(seat_ids) |> Summaries.records(),
      win_rate_over_time: Records.cumulative_win_rate(trend_games(entries), & &1),
      recent_games:
        Enum.map(
          recent_games,
          &Summaries.recent_game(
            &1,
            Enum.filter(&1.seats, fn seat -> MapSet.member?(tracked_seat_ids, seat.id) end)
          )
        )
    }
  end

  defp resolve(id) do
    direct_summaries = Catalog.card_summaries([{id, id}])

    case Catalog.card_summary(direct_summaries, id, id) do
      nil -> resolve_reference(id)
      summary -> canonical(summary, id, summary.name)
    end
  end

  defp resolve_reference(id) do
    references = Query.commander_references(id)
    summaries = Catalog.card_summaries(references)

    Enum.find_value(references, fn {stored_id, name} ->
      if stored_id == id or normalized_name(name) == normalized_name(id) do
        canonical(Catalog.card_summary(summaries, stored_id, name), stored_id, name)
      end
    end)
  end

  defp normalized_name(value) when is_binary(value), do: CardData.normalize_name(value)
  defp normalized_name(_value), do: nil

  defp same_commander?({key, _card, _seat}, key), do: true
  defp same_commander?(_entry, _key), do: false

  defp trend_games(entries) do
    entries
    |> Enum.group_by(fn {_key, _card, seat} -> seat.game_id end)
    |> Enum.map(fn {_game_id, rows} ->
      {_key, _card, first_seat} = hd(rows)

      tracked_seats =
        rows
        |> Enum.map(fn {_key, _card, seat} -> seat end)
        |> Enum.uniq_by(& &1.id)

      %{first_seat.game | seats: tracked_seats}
    end)
    |> Enum.sort_by(&{DateTime.to_unix(&1.played_at), &1.id}, :desc)
    |> Enum.take(@trend_game_limit)
  end

  # One `{key, card, seat}` entry per commander card the seat's deck ran, where `key`
  # is the canonical identity and `card` carries the resolved name plus the stored
  # reference used to resolve legacy IDs.
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
      |> Summaries.commander(card)
      |> Map.merge(Records.record(seats))
      |> Map.merge(extra_fun.(seats))
    end)
    |> Enum.sort_by(&{-&1.games, -&1.win_rate, String.downcase(&1.name)})
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
