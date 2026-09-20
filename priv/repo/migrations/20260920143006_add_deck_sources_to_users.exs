defmodule TheGathering.Repo.Migrations.AddDeckSourcesToUsers do
  use Ecto.Migration

  def change do
    alter table(:users) do
      add :moxfield_username, :string
      add :archidekt_username, :string
      add :manavault_url, :string
    end
  end
end
