defmodule TheGathering.Repo.Migrations.AddGameChangerToCards do
  use Ecto.Migration

  def change do
    for name <- [:cards, :catalog_cards_staging, :card_printings] do
      alter table(name) do
        add :game_changer, :boolean, default: false, null: false
      end
    end
  end
end
