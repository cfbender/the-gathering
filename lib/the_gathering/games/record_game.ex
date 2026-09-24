defmodule TheGathering.Games.RecordGame do
  @moduledoc false

  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Accounts.User
  alias TheGathering.Games.{Deck, Game, GamePlayer}
  alias TheGathering.Repo

  def create(attrs, created_by_user_id \\ nil) do
    case external_identity(attrs) do
      {source, external_id} when is_binary(external_id) and external_id != "" ->
        case Repo.get_by(Game, source: source, external_id: external_id) do
          nil -> insert(attrs, created_by_user_id, source, external_id)
          game -> {:ok, get_game!(game.id)}
        end

      _identity ->
        insert(attrs, created_by_user_id)
    end
  end

  def find_or_create_by_external_id(source, external_id, attrs) do
    attrs
    |> Map.new()
    |> Map.merge(%{source: source, external_id: external_id})
    |> create()
  end

  def upsert_by_external_id(source, external_id, attrs) do
    attrs =
      attrs
      |> Map.new()
      |> Map.merge(%{source: source, external_id: external_id})

    case Repo.get_by(Game, source: source, external_id: external_id) do
      nil -> insert(attrs, nil, source, external_id)
      game -> __MODULE__.update(game, attrs)
    end
  end

  def update(%Game{} = game, attrs) do
    Repo.transaction(fn ->
      game
      |> park_seats(attrs)
      |> Game.changeset(attrs)
      |> validate_deck_ownership()
      |> Repo.update()
      |> preload_game_ok()
      |> case do
        {:ok, game} -> game
        {:error, changeset} -> Repo.rollback(changeset)
      end
    end)
  end

  # Ecto updates retained seats one row at a time, so swapping two seat numbers
  # would trip the (game_id, seat) unique index halfway through. When the payload
  # claims a seat number another row currently holds, move every existing row to
  # a negative seat first; rows the payload keeps are re-numbered by the
  # changeset, the rest are deleted. Payloads that keep seat numbers where they
  # are leave the rows untouched.
  defp park_seats(game, attrs) do
    game = Repo.preload(game, :seats)

    if seats_collide?(game.seats, value(attrs, :seats)) do
      from(seat in GamePlayer, where: seat.game_id == ^game.id, update: [set: [seat: -seat.seat]])
      |> Repo.update_all([])

      Repo.preload(game, :seats, force: true)
    else
      game
    end
  end

  defp seats_collide?(current, requested) when is_list(requested) do
    held = Map.new(current, &{&1.seat, &1.id})

    Enum.any?(requested, fn
      seat when is_map(seat) ->
        case Map.fetch(held, to_integer(value(seat, :seat))) do
          {:ok, id} -> id != to_integer(value(seat, :id))
          :error -> false
        end

      _other ->
        false
    end)
  end

  defp seats_collide?(_current, _requested), do: false

  defp to_integer(param) when is_binary(param) do
    case Integer.parse(param) do
      {number, ""} -> number
      _ -> param
    end
  end

  defp to_integer(param), do: param

  defp insert(attrs, created_by_user_id, source \\ nil, external_id \\ nil) do
    result =
      %Game{}
      |> Game.changeset(attrs)
      |> Game.put_created_by(created_by_user_id)
      |> Game.put_external_identity(source, external_id)
      |> validate_user_exists(:created_by_user_id)
      |> validate_deck_ownership()
      |> Repo.insert()
      |> preload_game_ok()

    case result do
      {:error, changeset} when not is_nil(source) ->
        if Keyword.has_key?(changeset.errors, :external_id) do
          {:ok, source |> game_by_external_id!(external_id) |> get_game!()}
        else
          result
        end

      _result ->
        result
    end
  end

  defp validate_deck_ownership(changeset) do
    mismatched? =
      changeset
      |> get_field(:seats, [])
      |> Enum.any?(fn
        %{deck_id: nil} ->
          false

        %{deck_id: deck_id, player_id: player_id} ->
          Repo.get_by(Deck, id: deck_id, player_id: player_id) == nil
      end)

    if mismatched?,
      do: add_error(changeset, :seats, "contains a deck that does not belong to its player"),
      else: changeset
  end

  # SQLite reports foreign-key violations without a constraint name, so Ecto
  # cannot translate them through foreign_key_constraint/3 on its own.
  defp validate_user_exists(changeset, field) do
    case get_field(changeset, field) do
      nil ->
        changeset

      user_id ->
        if Repo.exists?(from user in User, where: user.id == ^user_id),
          do: changeset,
          else: add_error(changeset, field, "does not exist")
    end
  end

  defp external_identity(attrs) do
    {value(attrs, :source, "manual"), value(attrs, :external_id)}
  end

  defp value(map, key, default \\ nil) do
    Map.get(map, key, Map.get(map, Atom.to_string(key), default))
  end

  defp game_by_external_id!(source, external_id),
    do: Repo.get_by!(Game, source: source, external_id: external_id)

  defp get_game!(%Game{id: id}), do: get_game!(id)

  defp get_game!(id),
    do: Game |> Repo.get!(id) |> Repo.preload(seats: [:player, :deck, :eliminated_by_player])

  defp preload_game_ok({:ok, game}),
    do: {:ok, Repo.preload(game, [seats: [:player, :deck]], force: true)}

  defp preload_game_ok(error), do: error
end
