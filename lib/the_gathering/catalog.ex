defmodule TheGathering.Catalog do
  @moduledoc "Local, offline-searchable catalog synchronized from Scryfall bulk data."

  import Ecto.Query

  alias TheGathering.Catalog.{Card, CardData, SyncServer, SyncState}
  alias TheGathering.Repo

  @default_limit 20
  @max_limit 50

  def search(query, opts \\ []) do
    normalized = query |> to_string() |> String.trim() |> CardData.normalize_name()
    limit = opts |> Keyword.get(:limit, @default_limit) |> min(@max_limit) |> max(1)
    commander = Keyword.get(opts, :commander)

    if normalized == "" do
      []
    else
      pattern = "%#{escape_like(normalized)}%"
      prefix = "#{escape_like(normalized)}%"

      Card
      |> where([card], fragment("? LIKE ? ESCAPE '\\'", card.normalized_name, ^pattern))
      |> commander_filter(commander)
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

  def sync_status do
    Repo.one(from state in SyncState, order_by: [desc: state.id], limit: 1) || %SyncState{}
  end

  def trigger_sync, do: SyncServer.trigger()

  defp commander_filter(query, true), do: where(query, [card], card.can_be_commander)
  defp commander_filter(query, false), do: where(query, [card], not card.can_be_commander)
  defp commander_filter(query, _value), do: query

  defp escape_like(value) do
    value
    |> String.replace("\\", "\\\\")
    |> String.replace("%", "\\%")
    |> String.replace("_", "\\_")
  end
end
