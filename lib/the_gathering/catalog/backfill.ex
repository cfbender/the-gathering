defmodule TheGathering.Catalog.Backfill do
  @moduledoc """
  Links imported decks and MVP cards to the local card catalog by name.

  Imports from Mythic Track or CSV record commander and MVP names but usually
  no Scryfall IDs, so the UI has no card art or reliable color identity for
  them. This pass is idempotent and only fills what is missing:

    * splits `"Commander || Partner (Partners)"` commander names into separate
      commander and partner fields (Mythic Track's partner notation);
    * sets `commander_card_id` / `partner_card_id` from an exact name match,
      falling back to the front face of a double-faced card;
    * fills an empty `color_identity` from the matched cards;
    * sets `mvp_card_id` for seats that only have `mvp_card_name`.

  It runs after each import and catalog sync, and can be triggered by an admin.
  """

  import Ecto.Query

  alias TheGathering.Catalog.{Card, CardData}
  alias TheGathering.Games.{Deck, GamePlayer}
  alias TheGathering.Repo

  @type summary :: %{
          decks_split: non_neg_integer(),
          decks_linked: non_neg_integer(),
          colors_filled: non_neg_integer(),
          mvps_linked: non_neg_integer(),
          unmatched: [String.t()]
        }

  @spec run() :: summary()
  def run do
    {:ok, summary} = Repo.transaction(&backfill/0)
    summary
  end

  defp backfill do
    decks = Repo.all(from deck in Deck, where: is_nil(deck.commander_card_id))

    deck_results = Enum.map(decks, &backfill_deck/1)

    seats =
      Repo.all(
        from seat in GamePlayer,
          where: is_nil(seat.mvp_card_id) and not is_nil(seat.mvp_card_name)
      )

    mvp_results = Enum.map(seats, &backfill_mvp/1)

    %{
      decks_split: Enum.count(deck_results, & &1.split),
      decks_linked: Enum.count(deck_results, & &1.linked),
      colors_filled: Enum.count(deck_results, & &1.colored),
      mvps_linked: Enum.count(mvp_results, & &1.linked),
      unmatched:
        (deck_results ++ mvp_results)
        |> Enum.flat_map(& &1.unmatched)
        |> Enum.uniq()
        |> Enum.sort()
    }
  end

  @doc """
  Splits Mythic Track's `"A || B (Partners)"` notation into `{commander, partner}`.
  Names without the separator come back unchanged with a `nil` partner.
  """
  @spec split_partners(String.t()) :: {String.t(), String.t() | nil}
  def split_partners(name) when is_binary(name) do
    case String.split(name, "||", parts: 2) do
      [commander, partner] ->
        {String.trim(commander),
         partner |> String.replace(~r/\s*\([^)]*\)\s*$/, "") |> String.trim()}

      [only] ->
        {String.trim(only), nil}
    end
  end

  @doc "Finds the catalog card for a printed name, preferring commanders and current printings."
  @spec find_by_name(String.t()) :: Card.t() | nil
  def find_by_name(name) when is_binary(name) do
    normalized = CardData.normalize_name(name)

    exact =
      from card in Card,
        where: card.normalized_name == ^normalized,
        order_by: [desc: card.can_be_commander, desc: card.released_at],
        limit: 1

    Repo.one(exact) || Repo.one(front_face_query(normalized))
  end

  # Mythic Track records only the front face of a double-faced commander; the
  # catalog stores "Front // Back". Alchemy rebalances ("A-Name") are skipped.
  defp front_face_query(normalized) do
    prefix = normalized <> " // "

    from card in Card,
      where:
        fragment("substr(?, 1, ?) = ?", card.normalized_name, ^String.length(prefix), ^prefix) and
          not like(card.name, "A-%"),
      order_by: [desc: card.can_be_commander, desc: card.released_at],
      limit: 1
  end

  defp backfill_deck(%Deck{} = deck) do
    {commander_name, split_partner} = split_partners(deck.commander_name)
    split? = not is_nil(split_partner)
    partner_name = deck.partner_name || split_partner

    commander = find_by_name(commander_name)
    partner = if partner_name, do: find_by_name(partner_name)

    color_identity =
      if blank?(deck.color_identity),
        do: color_identity([commander, partner]),
        else: deck.color_identity

    deck
    |> Ecto.Changeset.change(%{
      commander_name: commander_name,
      partner_name: partner_name,
      commander_card_id: commander && commander.id,
      partner_card_id: deck.partner_card_id || (partner && partner.id),
      color_identity: color_identity
    })
    |> Ecto.Changeset.change(name_changes(deck, split?, commander_name, partner_name))
    |> Repo.update!()

    %{
      split: split?,
      linked: not is_nil(commander),
      colored: blank?(deck.color_identity) and color_identity != "",
      unmatched: unmatched_names([{commander_name, commander}, {partner_name, partner}])
    }
  end

  # A deck named after its piped commander string gets the importer's usual
  # "Commander / Partner" name; custom deck names are left alone.
  defp name_changes(%Deck{name: name, commander_name: name}, true, commander_name, partner_name),
    do: %{name: "#{commander_name} / #{partner_name}"}

  defp name_changes(_deck, _split?, _commander_name, _partner_name), do: %{}

  defp unmatched_names(pairs) do
    for {name, nil} when not is_nil(name) <- pairs, do: name
  end

  defp blank?(value), do: value in [nil, ""]

  defp backfill_mvp(%GamePlayer{} = seat) do
    case find_by_name(seat.mvp_card_name) do
      nil ->
        %{linked: false, unmatched: [seat.mvp_card_name]}

      card ->
        seat |> Ecto.Changeset.change(mvp_card_id: card.id) |> Repo.update!()
        %{linked: true, unmatched: []}
    end
  end

  defp color_identity(cards) do
    colors =
      cards
      |> Enum.reject(&is_nil/1)
      |> Enum.flat_map(&(&1.color_identity || []))

    ~w(W U B R G) |> Enum.filter(&(&1 in colors)) |> Enum.join()
  end
end
