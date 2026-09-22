defmodule TheGathering.Catalog do
  @moduledoc "Local, offline-searchable catalog synchronized from Scryfall bulk data."

  import Ecto.Query

  alias TheGathering.Catalog.{
    Backfill,
    Card,
    CardData,
    Printing,
    Printings,
    SyncServer,
    SyncState
  }

  alias TheGathering.Games.ColorIdentity
  alias TheGathering.Repo

  @default_limit 20
  @max_limit 50

  def search(query, opts \\ []) do
    normalized =
      query
      |> to_string()
      |> String.trim()
      |> CardData.normalize_name()
      |> strip_search_punctuation()

    limit = opts |> Keyword.get(:limit, @default_limit) |> min(@max_limit) |> max(1)
    commander = Keyword.get(opts, :commander)
    partner = Keyword.get(opts, :partner)

    if normalized == "" do
      []
    else
      pattern = "%#{escape_like(normalized)}%"
      prefix = "#{escape_like(normalized)}%"
      whole_name_prefix = "#{escape_like(normalized)} %"

      Card
      |> where(
        [card],
        fragment(
          "replace(replace(replace(?, '''', ''), '’', ''), ',', '') LIKE ? ESCAPE '\\'",
          card.normalized_name,
          ^pattern
        )
      )
      |> commander_filter(commander)
      |> partner_filter(partner)
      |> order_by(
        [card],
        asc:
          fragment(
            "CASE WHEN replace(replace(replace(?, '''', ''), '’', ''), ',', '') = ? THEN 0 WHEN replace(replace(replace(?, '''', ''), '’', ''), ',', '') LIKE ? ESCAPE '\\' THEN 1 WHEN replace(replace(replace(?, '''', ''), '’', ''), ',', '') LIKE ? ESCAPE '\\' THEN 2 ELSE 3 END",
            card.normalized_name,
            ^normalized,
            card.normalized_name,
            ^whole_name_prefix,
            card.normalized_name,
            ^prefix
          ),
        asc: card.normalized_name,
        asc: card.id
      )
      |> limit(^limit)
      |> Repo.all()
    end
  end

  def get_card(id), do: Repo.get(Card, id)
  def get_card!(id), do: Repo.get!(Card, id)
  def count_cards, do: Repo.aggregate(Card, :count)

  def resolve_card(id, name) do
    (id && get_card(id)) || (is_binary(name) && find_card_by_name(name)) || nil
  end

  def get_printing(id), do: Repo.get(Printing, id)

  def list_printings(id, name, page) when is_integer(page) and page > 0 do
    case resolve_card(id, name) do
      nil -> {:error, :not_found}
      card -> Printings.list(card, page)
    end
  end

  @doc "Finds a catalog card by printed name, preferring commanders and current printings."
  def find_card_by_name(name) when is_binary(name) do
    normalized = CardData.normalize_name(name)

    exact =
      from card in Card,
        where: card.normalized_name == ^normalized,
        order_by: [desc: card.can_be_commander, desc: card.released_at],
        limit: 1

    Repo.one(exact) || Repo.one(front_face_query(normalized))
  end

  @doc """
  Resolves `{card_id, card_name}` references to catalog summaries in one query.

  Returns a map keyed by `{:id, card_id}` and `{:name, normalized_name}` whose values are
  `%{id, name, art_crop_url, color_identity}`; look entries up with `card_summary/3`.
  Stored IDs win, and names cover legacy imported decks that only recorded a snapshot.
  """
  def card_summaries(card_refs) do
    ids = card_refs |> Enum.map(&elem(&1, 0)) |> Enum.reject(&is_nil/1) |> Enum.uniq()

    names =
      card_refs
      |> Enum.map(&elem(&1, 1))
      |> Enum.reject(&is_nil/1)
      |> Enum.map(&CardData.normalize_name/1)
      |> Enum.uniq()

    Card
    |> where([card], card.id in ^ids or card.normalized_name in ^names)
    |> select(
      [card],
      {card.id, card.name, card.normalized_name, card.image_uris, card.color_identity}
    )
    |> Repo.all()
    |> Enum.reduce(%{}, fn {id, name, normalized_name, image_uris, color_identity}, summaries ->
      summary = %{
        id: id,
        name: name,
        art_crop_url: Map.get(image_uris || %{}, "art_crop"),
        color_identity: ColorIdentity.canonical(Enum.join(color_identity || []))
      }

      summaries
      |> Map.put({:id, id}, summary)
      |> Map.put({:name, normalized_name}, summary)
    end)
  end

  def card_summary(summaries, id, name) do
    Map.get(summaries, {:id, id}) ||
      (is_binary(name) && Map.get(summaries, {:name, CardData.normalize_name(name)})) || nil
  end

  def art_crop_urls(card_refs) do
    {printing_refs, identity_refs} = Enum.split_with(card_refs, &match?({:printing, _id}, &1))
    ids = Enum.map(printing_refs, &elem(&1, 1)) |> Enum.reject(&is_nil/1) |> Enum.uniq()

    urls =
      identity_refs
      |> card_summaries()
      |> Map.new(fn {key, summary} -> {key, summary.art_crop_url} end)

    Printing
    |> where([printing], printing.id in ^ids)
    |> Repo.all()
    |> Enum.reduce(urls, fn printing, acc ->
      Map.put(acc, {:printing, printing.id}, printing.image_uris["art_crop"])
    end)
  end

  def art_crop_url(urls, id, name, printing_id \\ nil) do
    Map.get(urls, {:printing, printing_id}) || Map.get(urls, {:id, id}) ||
      (is_binary(name) && Map.get(urls, {:name, CardData.normalize_name(name)})) || nil
  end

  def sync_status do
    Repo.one(from state in SyncState, order_by: [desc: state.id], limit: 1) || %SyncState{}
  end

  def trigger_sync, do: SyncServer.trigger()

  @doc "Links imported decks and MVP cards to catalog cards by name. See `Backfill`."
  def backfill, do: Backfill.run()

  defp commander_filter(query, true), do: where(query, [card], card.can_be_commander)
  defp commander_filter(query, false), do: where(query, [card], not card.can_be_commander)
  defp commander_filter(query, _value), do: query

  defp partner_filter(query, true),
    do: where(query, [card], not is_nil(card.commander_pairing))

  defp partner_filter(query, _value), do: query

  defp front_face_query(normalized) do
    prefix = normalized <> " // "

    from card in Card,
      where:
        fragment("substr(?, 1, ?) = ?", card.normalized_name, ^String.length(prefix), ^prefix) and
          not like(card.name, "A-%"),
      order_by: [desc: card.can_be_commander, desc: card.released_at],
      limit: 1
  end

  defp escape_like(value) do
    value
    |> String.replace("\\", "\\\\")
    |> String.replace("%", "\\%")
    |> String.replace("_", "\\_")
  end

  defp strip_search_punctuation(value), do: String.replace(value, ["'", "’", ","], "")
end
