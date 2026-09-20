defmodule TheGathering.Catalog do
  @moduledoc "Local, offline-searchable catalog synchronized from Scryfall bulk data."

  import Ecto.Query

  alias TheGathering.Catalog.{Backfill, Card, CardData, SyncServer, SyncState}
  alias TheGathering.Games.ColorIdentity
  alias TheGathering.Repo

  @default_limit 20
  @max_limit 50

  def search(query, opts \\ []) do
    normalized = query |> to_string() |> String.trim() |> CardData.normalize_name()
    limit = opts |> Keyword.get(:limit, @default_limit) |> min(@max_limit) |> max(1)
    commander = Keyword.get(opts, :commander)
    partner = Keyword.get(opts, :partner)

    if normalized == "" do
      []
    else
      pattern = "%#{escape_like(normalized)}%"
      prefix = "#{escape_like(normalized)}%"

      Card
      |> where([card], fragment("? LIKE ? ESCAPE '\\'", card.normalized_name, ^pattern))
      |> commander_filter(commander)
      |> partner_filter(partner)
      |> order_by(
        [card],
        asc:
          fragment(
            "CASE WHEN ? = ? THEN 0 WHEN ? LIKE ? ESCAPE '\\' THEN 1 ELSE 2 END",
            card.normalized_name,
            ^normalized,
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
    card_refs
    |> card_summaries()
    |> Map.new(fn {key, summary} -> {key, summary.art_crop_url} end)
  end

  def art_crop_url(urls, id, name) do
    Map.get(urls, {:id, id}) ||
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

  defp escape_like(value) do
    value
    |> String.replace("\\", "\\\\")
    |> String.replace("%", "\\%")
    |> String.replace("_", "\\_")
  end
end
