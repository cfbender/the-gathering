defmodule TheGathering.Repo.Migrations.AddDiscordOauthToUsers do
  use Ecto.Migration

  def up do
    alter table(:users) do
      add :oauth_hashed_password, :string
      add :discord_id, :string
      add :avatar_url, :string
    end

    execute("UPDATE users SET oauth_hashed_password = hashed_password")

    alter table(:users) do
      remove :hashed_password
    end

    rename(table(:users), :oauth_hashed_password, to: :hashed_password)
    create unique_index(:users, [:discord_id])
  end

  def down do
    drop(index(:users, [:discord_id]))

    alter table(:users) do
      add :required_hashed_password, :string, null: false, default: ""
    end

    execute("UPDATE users SET required_hashed_password = COALESCE(hashed_password, '')")

    alter table(:users) do
      remove :hashed_password
      remove :discord_id
      remove :avatar_url
    end

    rename(table(:users), :required_hashed_password, to: :hashed_password)
  end
end
