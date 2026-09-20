defmodule TheGathering.Repo.Migrations.AddDeckChooserFields do
  use Ecto.Migration

  def change do
    alter table(:decks) do
      add :skip_count, :integer, null: false, default: 0
      add :included_for_play, :boolean, null: false, default: true
    end
  end
end
