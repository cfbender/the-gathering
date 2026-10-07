defmodule TheGathering.Repo.Migrations.RecomputeCanBeCommander do
  use Ecto.Migration

  import Ecto.Query

  alias TheGathering.Catalog.CardData

  # Commander eligibility now follows Comprehensive Rules 903.3 (legendary creature, Vehicle,
  # or Spacecraft judged by the front face, or "can be your commander" text), matching the
  # shared lotus crate. Recompute the stored flag so existing catalogs don't wait for the next
  # Scryfall sync. The Rust server ports this step in `db::migrate::data_step`.
  def up do
    from(card in "cards",
      select: {card.id, card.type_line, card.oracle_text, card.can_be_commander}
    )
    |> repo().all()
    |> Enum.each(fn {id, type_line, oracle_text, current} ->
      eligible = CardData.can_be_commander?(type_line || "", oracle_text || "")

      if eligible != current in [true, 1] do
        # Schemaless updates bind raw Elixir values, and SQLite would store `true` as the
        # text "true"; write the integers Ecto's :boolean type stores.
        repo().update_all(from(card in "cards", where: card.id == ^id),
          set: [can_be_commander: if(eligible, do: 1, else: 0)]
        )
      end
    end)
  end

  def down, do: :ok
end
