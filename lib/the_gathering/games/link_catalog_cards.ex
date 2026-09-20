defmodule TheGathering.Games.LinkCatalogCards do
  @moduledoc false

  import Ecto.Query

  alias TheGathering.Catalog
  alias TheGathering.Games.{Deck, GamePlayer}
  alias TheGathering.Repo

  @default_batch_size 100
  @max_batch_size 500

  @empty_summary %{
    decks_split: 0,
    decks_linked: 0,
    colors_filled: 0,
    mvps_linked: 0,
    unmatched: []
  }

  def link_game(game_id) do
    Repo.transaction(fn ->
      deck_ids =
        Repo.all(
          from seat in GamePlayer,
            where: seat.game_id == ^game_id and not is_nil(seat.deck_id),
            select: seat.deck_id,
            distinct: true
        )

      decks =
        Repo.all(
          from deck in Deck,
            where: deck.id in ^deck_ids and is_nil(deck.commander_card_id)
        )

      seats = Repo.all(from seat in GamePlayer, where: seat.game_id == ^game_id)
      link_rows(decks, seats)
    end)
  end

  def repair_batch(cursor \\ %{deck_id: 0, seat_id: 0}, opts \\ []) do
    limit = opts |> Keyword.get(:limit, @default_batch_size) |> min(@max_batch_size) |> max(1)

    Repo.transaction(fn ->
      decks =
        Repo.all(
          from deck in Deck,
            where: deck.id > ^cursor.deck_id and is_nil(deck.commander_card_id),
            order_by: [asc: deck.id],
            limit: ^limit
        )

      seats =
        Repo.all(
          from seat in GamePlayer,
            where:
              seat.id > ^cursor.seat_id and is_nil(seat.mvp_card_id) and
                not is_nil(seat.mvp_card_name),
            order_by: [asc: seat.id],
            limit: ^limit
        )

      result = link_rows(decks, seats)

      Map.merge(result, %{
        cursor: %{
          deck_id: last_id(decks, cursor.deck_id),
          seat_id: last_id(seats, cursor.seat_id)
        },
        done?: length(decks) < limit and length(seats) < limit
      })
    end)
  end

  def empty_summary, do: @empty_summary

  def merge_summaries(left, right) do
    %{
      decks_split: left.decks_split + right.decks_split,
      decks_linked: left.decks_linked + right.decks_linked,
      colors_filled: left.colors_filled + right.colors_filled,
      mvps_linked: left.mvps_linked + right.mvps_linked,
      unmatched: Enum.sort(Enum.uniq(left.unmatched ++ right.unmatched))
    }
  end

  def split_partners(name) when is_binary(name) do
    case String.split(name, "||", parts: 2) do
      [commander, partner] ->
        {String.trim(commander),
         partner |> String.replace(~r/\s*\([^)]*\)\s*$/, "") |> String.trim()}

      [only] ->
        {String.trim(only), nil}
    end
  end

  defp link_rows(decks, seats) do
    deck_results = Enum.map(decks, &link_deck/1)
    mvp_results = Enum.map(seats, &link_mvp/1)
    results = deck_results ++ mvp_results

    %{
      summary: %{
        decks_split: Enum.count(deck_results, & &1.split),
        decks_linked: Enum.count(deck_results, & &1.linked),
        colors_filled: Enum.count(deck_results, & &1.colored),
        mvps_linked: Enum.count(mvp_results, & &1.linked),
        unmatched: results |> Enum.flat_map(& &1.unmatched) |> Enum.uniq() |> Enum.sort()
      },
      conflicts: Enum.flat_map(results, & &1.conflicts)
    }
  end

  defp link_deck(%Deck{} = deck) do
    {commander_name, split_partner} = split_partners(deck.commander_name)
    split? = not is_nil(split_partner)
    partner_name = deck.partner_name || split_partner
    commander = Catalog.find_card_by_name(commander_name)
    partner = if partner_name, do: Catalog.find_card_by_name(partner_name)
    color_identity = deck_color_identity(deck, commander, partner)

    attrs = deck_attrs(deck, commander_name, partner_name, commander, partner, color_identity)

    context = %{
      deck: deck,
      split?: split?,
      commander: commander,
      partner: partner,
      commander_name: commander_name,
      partner_name: partner_name,
      color_identity: color_identity
    }

    deck
    |> Deck.update_changeset(
      Map.merge(attrs, name_changes(deck, split?, commander_name, partner_name))
    )
    |> Repo.update()
    |> link_deck_result(context)
  end

  defp link_deck_result(result, context) do
    case result do
      {:ok, _deck} ->
        %{
          split: context.split?,
          linked: not is_nil(context.commander),
          colored: blank?(context.deck.color_identity) and context.color_identity != "",
          unmatched:
            unmatched_names([
              {context.commander_name, context.commander},
              {context.partner_name, context.partner}
            ]),
          conflicts: []
        }

      {:error, changeset} ->
        %{
          split: false,
          linked: false,
          colored: false,
          unmatched:
            unmatched_names([
              {context.commander_name, context.commander},
              {context.partner_name, context.partner}
            ]),
          conflicts: [%{resource: :deck, id: context.deck.id, errors: changeset.errors}]
        }
    end
  end

  defp deck_color_identity(deck, commander, partner) do
    if blank?(deck.color_identity),
      do: color_identity([commander, partner]),
      else: deck.color_identity
  end

  defp deck_attrs(deck, commander_name, partner_name, commander, partner, color_identity) do
    %{
      commander_name: commander_name,
      partner_name: partner_name,
      commander_card_id: commander && commander.id,
      partner_card_id: deck.partner_card_id || (partner && partner.id),
      color_identity: color_identity
    }
  end

  defp link_mvp(%GamePlayer{mvp_card_id: nil, mvp_card_name: name} = seat)
       when not is_nil(name) do
    case Catalog.find_card_by_name(name) do
      nil ->
        %{linked: false, unmatched: [name], conflicts: []}

      card ->
        case seat |> Ecto.Changeset.change(mvp_card_id: card.id) |> Repo.update() do
          {:ok, _seat} ->
            %{linked: true, unmatched: [], conflicts: []}

          {:error, changeset} ->
            %{
              linked: false,
              unmatched: [],
              conflicts: [%{resource: :game_player, id: seat.id, errors: changeset.errors}]
            }
        end
    end
  end

  defp link_mvp(%GamePlayer{}), do: %{linked: false, unmatched: [], conflicts: []}

  defp name_changes(%Deck{name: name, commander_name: name}, true, commander_name, partner_name),
    do: %{name: "#{commander_name} / #{partner_name}"}

  defp name_changes(_deck, _split?, _commander_name, _partner_name), do: %{}

  defp unmatched_names(pairs), do: for({name, nil} when not is_nil(name) <- pairs, do: name)
  defp blank?(value), do: value in [nil, ""]

  defp color_identity(cards) do
    colors =
      cards
      |> Enum.reject(&is_nil/1)
      |> Enum.flat_map(&(&1.color_identity || []))

    ~w(W U B R G) |> Enum.filter(&(&1 in colors)) |> Enum.join()
  end

  defp last_id([], fallback), do: fallback
  defp last_id(rows, _fallback), do: List.last(rows).id
end
