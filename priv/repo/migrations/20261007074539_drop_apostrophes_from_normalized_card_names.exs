defmodule TheGathering.Repo.Migrations.DropApostrophesFromNormalizedCardNames do
  use Ecto.Migration

  # Card names are now normalized with the shared lotus crate's rule (also used by the
  # Rust server): apostrophes are dropped and whitespace runs collapse, so "Aurelia's
  # Fury" is stored as "aurelias fury". Card names never contain whitespace runs, so
  # removing both apostrophe forms brings stored values in line; the Rust migrator
  # recomputes every name exactly as well.
  def up do
    for table <- ~w(cards catalog_cards_staging) do
      execute(
        "UPDATE #{table} SET normalized_name = replace(replace(normalized_name, '''', ''), '’', '')"
      )
    end
  end

  def down, do: :ok
end
