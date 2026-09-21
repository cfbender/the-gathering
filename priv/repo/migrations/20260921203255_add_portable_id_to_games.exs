defmodule TheGathering.Repo.Migrations.AddPortableIdToGames do
  use Ecto.Migration
  import Ecto.Query

  def up do
    alter table(:games) do
      add :portable_id, :uuid
    end

    flush()

    repo().query!("SELECT id FROM games").rows
    |> Enum.each(fn [id] ->
      uuid = Ecto.UUID.generate()

      repo().update_all(
        from(g in "games",
          where: g.id == ^id,
          update: [set: [portable_id: type(^uuid, Ecto.UUID)]]
        ),
        []
      )
    end)

    create unique_index(:games, [:portable_id])
  end

  def down do
    drop index(:games, [:portable_id])

    alter table(:games) do
      remove :portable_id
    end
  end
end
