defmodule TheGathering.Games.Player do
  use Ecto.Schema
  import Ecto.Changeset

  schema "players" do
    field :name, :string
    field :discord_id, :string
    field :archived_at, :utc_datetime
    # Populated from the linked user by `Games.list_players/1` and `Games.get_player!/1`.
    field :avatar_url, :string, virtual: true

    belongs_to :user, TheGathering.Accounts.User
    has_many :decks, TheGathering.Games.Deck
    has_many :game_players, TheGathering.Games.GamePlayer

    timestamps(type: :utc_datetime)
  end

  def changeset(player, attrs) do
    player
    |> cast(attrs, [:name, :archived_at])
    |> update_change(:name, &String.trim/1)
    |> validate_required([:name])
    |> validate_length(:name, min: 1, max: 100)
    |> unique_constraint(:name, name: :players_name_nocase_index)
    |> unique_constraint(:name)
    |> unique_constraint(:user_id)
    |> unique_constraint(:discord_id)
    |> foreign_key_constraint(:user_id)
  end

  def put_user(changeset, user_id), do: put_change(changeset, :user_id, user_id)
  def put_discord_id(changeset, nil), do: changeset
  def put_discord_id(changeset, discord_id), do: put_change(changeset, :discord_id, discord_id)
end
