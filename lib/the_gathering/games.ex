defmodule TheGathering.Games do
  @moduledoc "Domain context for players, decks, games, and import-friendly identity matching."

  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Accounts.User
  alias TheGathering.Games.{Deck, Game, GamePlayer, Player}
  alias TheGathering.Repo

  def list_players(opts \\ %{}) do
    include_archived = value(opts, :include_archived, false)

    Player
    |> maybe_active(include_archived)
    |> with_avatar()
    |> order_by([player], asc: fragment("lower(?)", player.name))
    |> Repo.all()
  end

  def get_player!(id) do
    Player
    |> where(id: ^id)
    |> with_avatar()
    |> Repo.one!()
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

  def create_player(attrs, user_id \\ nil) do
    %Player{}
    |> Player.changeset(attrs)
    |> Player.put_user(user_id)
    |> validate_user_exists(:user_id)
    |> Repo.insert()
  end

  def update_player(%Player{} = player, attrs),
    do: player |> Player.changeset(attrs) |> Repo.update()

  def delete_player(%Player{} = player), do: Repo.delete(player)

  @doc """
  Case-folds a player or deck name the way SQLite compares them.

  The `players_name_nocase_index` and `decks_player_name_nocase_index` unique
  indexes use `COLLATE NOCASE`, and `lower()` in queries is likewise ASCII-only.
  Unicode-aware `String.downcase/1` turns `Éowyn` into `éowyn` while the database
  leaves it unchanged, so a lookup would miss the existing row and the insert would
  then hit the index. Use this for every case-insensitive name comparison that has
  to agree with the database.
  """
  def fold_name(name) when is_binary(name), do: name |> String.trim() |> String.downcase(:ascii)

  def find_or_create_player_by_name(name, attrs \\ %{}) when is_binary(name) do
    case Repo.one(
           from player in Player,
             where: fragment("lower(?)", player.name) == ^fold_name(name)
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

  @doc """
  Folds `source` into `target`: every seat and deck moves to `target`, the
  account/Discord identity is carried over, and `source` is deleted.

  Decks with the same name (case-insensitive) collapse into `target`'s deck.
  Fails with a changeset error when the two players sat in the same game or
  belong to different accounts, since neither can be represented afterwards.
  """
  def merge_players(%Player{id: id}, %Player{id: id}), do: {:error, :bad_request}

  def merge_players(%Player{} = source, %Player{} = target) do
    Repo.transaction(fn ->
      with :ok <- ensure_mergeable(source, target),
           {:ok, target} <- carry_identity(source, target) do
        move_decks(source, target)

        Repo.update_all(from(seat in GamePlayer, where: seat.player_id == ^source.id),
          set: [player_id: target.id]
        )

        Repo.delete!(source)
        Repo.get!(Player, target.id)
      else
        {:error, reason} -> Repo.rollback(reason)
      end
    end)
  end

  @doc """
  Makes `player` the account's player. The account's current player, if any and
  different, is merged into `player` so no game history is lost.
  """
  def link_player_to_user(%Player{} = player, %User{} = user) do
    player = Repo.get!(Player, player.id)
    current = Repo.get_by(Player, user_id: user.id)

    cond do
      current && current.id == player.id ->
        {:ok, player}

      conflicting?(player.user_id, user.id) ->
        {:error, merge_error(player, "players belong to different accounts")}

      conflicting?(player.discord_id, user.discord_id) ->
        {:error, merge_error(player, "players have different Discord identities")}

      current ->
        merge_players(current, player)

      true ->
        player
        |> change(user_id: user.id)
        |> maybe_put_discord_id(user.discord_id)
        |> Repo.update()
    end
  end

  defp ensure_mergeable(source, target) do
    shared_games =
      Repo.exists?(
        from a in GamePlayer,
          join: b in GamePlayer,
          on: a.game_id == b.game_id,
          where: a.player_id == ^source.id and b.player_id == ^target.id
      )

    cond do
      shared_games ->
        {:error, merge_error(source, "both players are seated in the same game")}

      conflicting?(source.user_id, target.user_id) ->
        {:error, merge_error(source, "players belong to different accounts")}

      conflicting?(source.discord_id, target.discord_id) ->
        {:error, merge_error(source, "players have different Discord identities")}

      true ->
        :ok
    end
  end

  defp conflicting?(a, b), do: not is_nil(a) and not is_nil(b) and a != b

  defp merge_error(source, message), do: source |> change() |> add_error(:merge, message)

  defp carry_identity(source, target) do
    # Free the unique columns on the source first so the target can take them.
    source |> change(user_id: nil, discord_id: nil) |> Repo.update!()

    target
    |> change(user_id: target.user_id || source.user_id)
    |> maybe_put_discord_id(source.discord_id)
    |> Repo.update()
  end

  defp maybe_put_discord_id(changeset, nil), do: changeset

  defp maybe_put_discord_id(changeset, discord_id) do
    case get_field(changeset, :discord_id) do
      nil -> put_change(changeset, :discord_id, discord_id)
      _existing -> changeset
    end
  end

  defp move_decks(source, target) do
    target_decks =
      Repo.all(from deck in Deck, where: deck.player_id == ^target.id)
      |> Map.new(&{fold_name(&1.name), &1})

    for deck <- Repo.all(from deck in Deck, where: deck.player_id == ^source.id) do
      case Map.fetch(target_decks, fold_name(deck.name)) do
        {:ok, existing} ->
          Repo.update_all(from(seat in GamePlayer, where: seat.deck_id == ^deck.id),
            set: [deck_id: existing.id]
          )

          Repo.delete!(deck)

        :error ->
          deck |> change(player_id: target.id) |> Repo.update!()
      end
    end

    :ok
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
        where: fragment("lower(?)", deck.name) == ^fold_name(name)

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

  def create_game(attrs, created_by_user_id \\ nil) do
    case external_identity(attrs) do
      {source, external_id} when is_binary(external_id) and external_id != "" ->
        case Repo.get_by(Game, source: source, external_id: external_id) do
          nil -> insert_game(attrs, created_by_user_id, source, external_id)
          game -> {:ok, get_game!(game.id)}
        end

      _identity ->
        insert_game(attrs, created_by_user_id)
    end
  end

  def find_or_create_game_by_external_id(source, external_id, attrs) do
    attrs
    |> Map.new()
    |> Map.merge(%{source: source, external_id: external_id})
    |> create_game()
  end

  def upsert_game_by_external_id(source, external_id, attrs) do
    attrs =
      attrs
      |> Map.new()
      |> Map.merge(%{source: source, external_id: external_id})

    case Repo.get_by(Game, source: source, external_id: external_id) do
      nil -> insert_game(attrs, nil, source, external_id)
      game -> update_game(game, attrs)
    end
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

  defp insert_game(attrs, created_by_user_id, source \\ nil, external_id \\ nil) do
    result =
      %Game{}
      |> Game.changeset(attrs)
      |> Game.put_created_by(created_by_user_id)
      |> validate_user_exists(:created_by_user_id)
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

  defp recover_player({:error, _changeset} = error, name) do
    case Repo.one(
           from player in Player,
             where: fragment("lower(?)", player.name) == ^fold_name(name)
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

  defp with_avatar(query) do
    from player in query,
      left_join: user in assoc(player, :user),
      select_merge: %{avatar_url: user.avatar_url}
  end

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
