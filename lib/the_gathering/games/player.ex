defmodule TheGathering.Games.Player do
  use Ecto.Schema
  import Ecto.Changeset

  schema "players" do
    field :name, :string
    field :user_id, :integer
    field :discord_id, :string
    field :archived_at, :utc_datetime
    # Populated from the linked user by `Games.list_players/1` and `Games.get_player!/1`.
    field :avatar_url, :string, virtual: true

    has_many :decks, TheGathering.Games.Deck
    has_many :game_players, TheGathering.Games.GamePlayer

    timestamps(type: :utc_datetime)
  end

  def changeset(player, attrs) do
    player
    |> cast(attrs, [:name, :user_id, :discord_id, :archived_at])
    |> update_change(:name, &String.trim/1)
    |> validate_required([:name])
    |> validate_length(:name, min: 1, max: 100)
    |> unique_constraint(:name, name: :players_name_nocase_index)
    |> unique_constraint(:name)
    |> unique_constraint(:user_id)
    |> unique_constraint(:discord_id)
  end
end
