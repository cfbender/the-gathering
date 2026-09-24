defmodule TheGathering.Repo.Migrations.AddAppearanceToUsers do
  use Ecto.Migration

  def change do
    alter table(:users) do
      add :palette, :string, null: false, default: "claret"
      add :theme_style, :string, null: false, default: "glass"
    end
  end
end
