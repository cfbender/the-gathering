defmodule TheGathering.Repo.Migrations.CreateGameTrackingTables do
  use Ecto.Migration

  def change do
    create table(:players) do
      add :name, :string, null: false
      add :user_id, :integer
      add :discord_id, :string
      add :archived_at, :utc_datetime

      timestamps(type: :utc_datetime)
    end

    create unique_index(:players, [:user_id])
    create unique_index(:players, [:discord_id])

    execute(
      "CREATE UNIQUE INDEX players_name_nocase_index ON players (name COLLATE NOCASE)",
      "DROP INDEX players_name_nocase_index"
    )

    create table(:decks) do
      add :player_id, references(:players, on_delete: :restrict), null: false
      add :name, :string, null: false
      add :commander_card_id, :string
      add :commander_name, :string, null: false
      add :partner_card_id, :string
      add :partner_name, :string
      add :color_identity, :string, null: false, default: ""
      add :decklist_url, :string
      add :decklist_source, :string
      add :archived_at, :utc_datetime

      timestamps(type: :utc_datetime)
    end

    create index(:decks, [:player_id])

    execute(
      "CREATE UNIQUE INDEX decks_player_name_nocase_index ON decks (player_id, name COLLATE NOCASE)",
      "DROP INDEX decks_player_name_nocase_index"
    )

    create table(:games) do
      add :played_at, :utc_datetime, null: false
      add :duration_minutes, :integer
      add :turns, :integer
      add :notes, :text
      add :source, :string, null: false, default: "manual"
      add :external_id, :string
      add :created_by_user_id, :integer

      timestamps(type: :utc_datetime)
    end

    create unique_index(:games, [:source, :external_id], where: "external_id IS NOT NULL")
    create index(:games, [:played_at])

    create table(:game_players) do
      add :game_id, references(:games, on_delete: :delete_all), null: false
      add :player_id, references(:players, on_delete: :restrict), null: false
      add :deck_id, references(:decks, on_delete: :restrict)
      add :seat, :integer, null: false
      add :result, :string, null: false
      add :eliminated_turn, :integer
      add :eliminated_by_player_id, references(:players, on_delete: :restrict)
      add :mvp_card_id, :string
      add :mvp_card_name, :string
      add :notes, :text

      timestamps(type: :utc_datetime)
    end

    create unique_index(:game_players, [:game_id, :player_id])
    create unique_index(:game_players, [:game_id, :seat])
    create index(:game_players, [:player_id])
    create index(:game_players, [:deck_id])
  end
end
