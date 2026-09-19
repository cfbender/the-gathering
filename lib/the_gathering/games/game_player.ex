defmodule TheGathering.Games.GamePlayer do
  use Ecto.Schema
  import Ecto.Changeset

  schema "game_players" do
    field :seat, :integer
    field :result, :string
    field :eliminated_turn, :integer
    field :mvp_card_id, :string
    field :mvp_card_name, :string
    field :notes, :string

    belongs_to :game, TheGathering.Games.Game
    belongs_to :player, TheGathering.Games.Player
    belongs_to :deck, TheGathering.Games.Deck
    belongs_to :eliminated_by_player, TheGathering.Games.Player

    timestamps(type: :utc_datetime)
  end

  def changeset(game_player, attrs) do
    game_player
    |> cast(attrs, [
      :id,
      :player_id,
      :deck_id,
      :seat,
      :result,
      :eliminated_turn,
      :eliminated_by_player_id,
      :mvp_card_id,
      :mvp_card_name,
      :notes
    ])
    |> validate_required([:player_id, :seat, :result])
    |> validate_number(:seat, greater_than_or_equal_to: 1, less_than_or_equal_to: 6)
    |> validate_number(:eliminated_turn, greater_than: 0)
    |> validate_inclusion(:result, ~w(win loss draw))
    |> assoc_constraint(:player)
    |> assoc_constraint(:deck)
    |> assoc_constraint(:eliminated_by_player)
    |> unique_constraint(:player_id, name: :game_players_game_id_player_id_index)
    |> unique_constraint(:seat, name: :game_players_game_id_seat_index)
  end
end
