defmodule TheGathering.Repo.Migrations.RecomputeCommanderPairings do
  use Ecto.Migration

  import Ecto.Query

  alias TheGathering.Catalog.CardData

  # Pairing detection used to miss reminder text ("Choose a Background (…)"),
  # "Partner—…" variants, Doctor's companion, and the Doctors. Recompute the
  # stored value from each card's type line and Oracle text so existing catalogs
  # don't have to wait for the next Scryfall sync.
  def up do
    from(card in "cards",
      select: {card.id, card.type_line, card.oracle_text, card.commander_pairing}
    )
    |> repo().all()
    |> Enum.each(fn {id, type_line, oracle_text, current} ->
      pairing = CardData.commander_pairing(type_line || "", oracle_text || "")

      if pairing != current do
        repo().update_all(from(card in "cards", where: card.id == ^id),
          set: [commander_pairing: pairing]
        )
      end
    end)
  end

  def down, do: :ok
end
