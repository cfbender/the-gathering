defmodule TheGathering.Games do
  @moduledoc "Domain context for players, decks, games, and import-friendly identity matching."

  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Games.{Deck, Game, GamePlayer, Player}
  alias TheGathering.Repo

  def list_players(opts \\ %{}) do
    include_archived = value(opts, :include_archived, false)

    Player
    |> maybe_active(include_archived)
    |> order_by([player], asc: fragment("lower(?)", player.name))
    |> Repo.all()
  end

  def get_player!(id) do
    Player
    |> Repo.get!(id)
    |> Repo.preload(
      decks: from(deck in Deck, order_by: [asc: deck.name]),
      game_players:
        from(seat in GamePlayer,
          join: game in assoc(seat, :game),
          order_by: [desc: game.played_at],
          preload: [:deck, game: game]
        )
    )
  end

  def get_player(id), do: Repo.get(Player, id)

  def create_player(attrs), do: %Player{} |> Player.changeset(attrs) |> Repo.insert()

  def update_player(%Player{} = player, attrs),
    do: player |> Player.changeset(attrs) |> Repo.update()

  def delete_player(%Player{} = player), do: Repo.delete(player)

  def find_or_create_player_by_name(name, attrs \\ %{}) when is_binary(name) do
    case Repo.one(
           from player in Player,
             where: fragment("lower(?)", player.name) == ^String.downcase(String.trim(name))
         ) do
      nil ->
        attrs
        |> Map.new()
        |> Map.put(:name, name)
        |> create_player()
        |> recover_player(name)

      player ->
        {:ok, player}
    end
  end

  def find_or_create_player_by_discord_id(discord_id, name) do
    case Repo.get_by(Player, discord_id: discord_id) do
      nil -> create_player(%{discord_id: discord_id, name: name})
      player -> {:ok, player}
    end
  end

  def list_decks(opts \\ %{}) do
    include_archived = value(opts, :include_archived, false)

    Deck
    |> maybe_active(include_archived)
    |> maybe_where_player(value(opts, :player_id))
    |> order_by([deck], asc: fragment("lower(?)", deck.name))
    |> preload(:player)
    |> Repo.all()
  end

  def get_deck!(id) do
    game_players =
      from seat in GamePlayer,
        join: game in assoc(seat, :game),
        order_by: [desc: game.played_at],
        preload: [game: game]

    Deck |> Repo.get!(id) |> Repo.preload([:player, game_players: game_players])
  end

  def get_deck(id), do: Repo.get(Deck, id)

  def create_deck(attrs),
    do: %Deck{} |> Deck.changeset(attrs) |> Repo.insert() |> preload_ok(:player)

  def update_deck(%Deck{} = deck, attrs),
    do: deck |> Deck.changeset(attrs) |> Repo.update() |> preload_ok(:player)

  def delete_deck(%Deck{} = deck), do: Repo.delete(deck)

  def find_or_create_deck(player_or_id, name, attrs \\ %{}) when is_binary(name) do
    player_id = if is_struct(player_or_id, Player), do: player_or_id.id, else: player_or_id

    query =
      from deck in Deck,
        where: deck.player_id == ^player_id,
        where: fragment("lower(?)", deck.name) == ^String.downcase(String.trim(name))

    case Repo.one(query) do
      nil ->
        attrs
        |> Map.new()
        |> Map.merge(%{player_id: player_id, name: name})
        |> create_deck()
        |> recover_deck(query)

      deck ->
        {:ok, deck}
    end
  end

  def list_games(opts \\ %{}) do
    page = positive_integer(value(opts, :page), 1)
    per_page = value(opts, :per_page) |> positive_integer(20) |> min(100)

    query =
      Game
      |> maybe_game_player(value(opts, :player_id))
      |> maybe_game_deck(value(opts, :deck_id))
      |> maybe_date_from(value(opts, :date_from))
      |> maybe_date_to(value(opts, :date_to))
      |> order_by([game], desc: game.played_at, desc: game.id)

    total = Repo.aggregate(query, :count, :id)

    games =
      query
      |> limit(^per_page)
      |> offset(^((page - 1) * per_page))
      |> preload(seats: [:player, :deck])
      |> Repo.all()

    {games,
     %{page: page, per_page: per_page, total: total, total_pages: max(ceil(total / per_page), 1)}}
  end

  def get_game!(id),
    do: Game |> Repo.get!(id) |> Repo.preload(seats: [:player, :deck, :eliminated_by_player])

  def get_game(id), do: Repo.get(Game, id)

  def create_game(attrs) do
    case external_identity(attrs) do
      {source, external_id} when is_binary(external_id) and external_id != "" ->
        case Repo.get_by(Game, source: source, external_id: external_id) do
          nil -> insert_game(attrs, source, external_id)
          game -> {:ok, get_game!(game.id)}
        end

      _identity ->
        insert_game(attrs)
    end
  end

  def find_or_create_game_by_external_id(source, external_id, attrs) do
    attrs
    |> Map.new()
    |> Map.merge(%{source: source, external_id: external_id})
    |> create_game()
  end

  def update_game(%Game{} = game, attrs) do
    game
    |> Repo.preload(:seats)
    |> Game.changeset(attrs)
    |> validate_deck_ownership()
    |> Repo.update()
    |> preload_game_ok()
  end

  def delete_game(%Game{} = game), do: Repo.delete(game)

  defp insert_game(attrs, source \\ nil, external_id \\ nil) do
    result =
      %Game{}
      |> Game.changeset(attrs)
      |> validate_deck_ownership()
      |> Repo.insert()
      |> preload_game_ok()

    case result do
      {:error, changeset} when not is_nil(source) ->
        if Keyword.has_key?(changeset.errors, :external_id) do
          {:ok,
           Game
           |> Repo.get_by!(source: source, external_id: external_id)
           |> then(&get_game!(&1.id))}
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

  defp external_identity(attrs) do
    {value(attrs, :source, "manual"), value(attrs, :external_id)}
  end

  defp recover_player({:error, _changeset} = error, name) do
    case Repo.one(
           from player in Player,
             where: fragment("lower(?)", player.name) == ^String.downcase(String.trim(name))
         ) do
      nil -> error
      player -> {:ok, player}
    end
  end

  defp recover_player(result, _name), do: result

  defp recover_deck({:error, _changeset} = error, query) do
    case Repo.one(query) do
      nil -> error
      deck -> {:ok, deck}
    end
  end

  defp recover_deck(result, _query), do: result

  defp preload_ok({:ok, struct}, association), do: {:ok, Repo.preload(struct, association)}
  defp preload_ok(error, _association), do: error

  defp preload_game_ok({:ok, game}),
    do: {:ok, Repo.preload(game, [seats: [:player, :deck]], force: true)}

  defp preload_game_ok(error), do: error

  defp maybe_active(query, true), do: query

  defp maybe_active(query, _include_archived),
    do: where(query, [resource], is_nil(resource.archived_at))

  defp maybe_where_player(query, nil), do: query

  defp maybe_where_player(query, player_id),
    do: where(query, [deck], deck.player_id == ^player_id)

  defp maybe_game_player(query, nil), do: query

  defp maybe_game_player(query, player_id) do
    game_ids = from seat in GamePlayer, where: seat.player_id == ^player_id, select: seat.game_id
    where(query, [game], game.id in subquery(game_ids))
  end

  defp maybe_game_deck(query, nil), do: query

  defp maybe_game_deck(query, deck_id) do
    game_ids = from seat in GamePlayer, where: seat.deck_id == ^deck_id, select: seat.game_id
    where(query, [game], game.id in subquery(game_ids))
  end

  defp maybe_date_from(query, nil), do: query

  defp maybe_date_from(query, date),
    do: where(query, [game], game.played_at >= ^start_of_day(date))

  defp maybe_date_to(query, nil), do: query
  defp maybe_date_to(query, date), do: where(query, [game], game.played_at <= ^end_of_day(date))

  defp start_of_day(%Date{} = date), do: DateTime.new!(date, ~T[00:00:00], "Etc/UTC")
  defp start_of_day(date), do: date |> Date.from_iso8601!() |> start_of_day()
  defp end_of_day(%Date{} = date), do: DateTime.new!(date, ~T[23:59:59], "Etc/UTC")
  defp end_of_day(date), do: date |> Date.from_iso8601!() |> end_of_day()

  defp positive_integer(value, _default) when is_integer(value) and value > 0, do: value

  defp positive_integer(value, default) when is_binary(value) do
    case Integer.parse(value) do
      {integer, ""} when integer > 0 -> integer
      _other -> default
    end
  end

  defp positive_integer(_value, default), do: default

  defp value(map, key, default \\ nil),
    do: Map.get(map, key, Map.get(map, Atom.to_string(key), default))
end
