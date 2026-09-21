defmodule TheGathering.Games.Game do
  use Ecto.Schema
  import Ecto.Changeset

  alias TheGathering.Games.{GamePlayer, WinCondition}

  schema "games" do
    field :played_at, :utc_datetime
    field :duration_minutes, :integer
    field :turns, :integer
    field :win_condition, :string
    field :notes, :string
    field :source, :string, default: "manual"
    field :external_id, :string
    field :portable_id, Ecto.UUID, autogenerate: true

    belongs_to :created_by, TheGathering.Accounts.User, foreign_key: :created_by_user_id
    has_many :seats, GamePlayer, on_replace: :delete

    timestamps(type: :utc_datetime)
  end

  def changeset(game, attrs) do
    game
    |> cast(attrs, [
      :played_at,
      :duration_minutes,
      :turns,
      :win_condition,
      :notes
    ])
    |> validate_required([:played_at, :source])
    |> validate_inclusion(:source, ~w(manual csv mythic_track discord))
    |> validate_inclusion(:win_condition, WinCondition.keys())
    |> validate_number(:duration_minutes, greater_than: 0)
    |> validate_number(:turns, greater_than: 0)
    |> cast_assoc(:seats, required: true, with: &GamePlayer.changeset/2)
    |> validate_seats()
    |> unique_constraint([:source, :external_id])
    |> unique_constraint(:portable_id)
    |> foreign_key_constraint(:created_by_user_id)
  end

  def put_created_by(changeset, user_id),
    do: put_change(changeset, :created_by_user_id, user_id)

  def put_external_identity(changeset, nil, nil), do: changeset

  def put_external_identity(changeset, source, external_id) do
    changeset
    |> put_change(:source, source)
    |> put_change(:external_id, external_id)
  end

  defp validate_seats(changeset) do
    seats = get_field(changeset, :seats, [])
    player_ids = Enum.map(seats, & &1.player_id)
    seat_numbers = Enum.map(seats, & &1.seat)
    winners = Enum.count(seats, &(&1.result == "win"))
    winner_and_losses = winners == 1 and Enum.all?(seats, &(&1.result in ~w(win loss)))
    all_draw = seats != [] and Enum.all?(seats, &(&1.result == "draw"))

    changeset
    |> then(fn changeset ->
      if length(seats) in 2..6,
        do: changeset,
        else: add_error(changeset, :seats, "must contain between 2 and 6 players")
    end)
    |> then(fn changeset ->
      if Enum.uniq(player_ids) == player_ids,
        do: changeset,
        else: add_error(changeset, :seats, "cannot contain the same player twice")
    end)
    |> then(fn changeset ->
      if Enum.sort(seat_numbers) == Enum.to_list(1..length(seats)),
        do: changeset,
        else: add_error(changeset, :seats, "must use consecutive seat numbers starting at 1")
    end)
    |> then(fn changeset ->
      if winner_and_losses or all_draw,
        do: changeset,
        else: add_error(changeset, :seats, "must have exactly one winner or all draws")
    end)
  end
end
