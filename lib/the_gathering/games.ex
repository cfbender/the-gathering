defmodule TheGathering.Games do
  @moduledoc "Domain context for players, decks, games, and import-friendly identity matching."

  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Accounts.User

  alias TheGathering.Games.{
    Deck,
    DeckPicker,
    Game,
    GamePlayer,
    ManaVaultSync,
    MergePlayers,
    Player,
    RecordGame,
    ResolvePlayer
  }

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
    discord_id = value(attrs, :discord_id)

    %Player{}
    |> Player.changeset(attrs)
    |> Player.put_user(user_id)
    |> Player.put_discord_id(discord_id)
    |> validate_user_exists(:user_id)
    |> Repo.insert()
  end

  @doc """
  Whether `user` may edit or remove `player` and that player's decks.

  Administrators manage everyone. Members manage their own linked player and
  unclaimed guests (players without an account), since nobody else could keep
  those imported or Discord-only players tidy. Another member's player is
  off-limits.
  """
  def can_manage_player?(%User{role: "admin"}, _player), do: true
  def can_manage_player?(%User{}, %Player{user_id: nil}), do: true
  def can_manage_player?(%User{id: id}, %Player{user_id: id}), do: true
  def can_manage_player?(_user, _player), do: false

  def can_manage_deck?(%User{role: "admin"}, %Deck{}), do: true

  def can_manage_deck?(%User{id: user_id}, %Deck{player_id: player_id}) do
    Repo.exists?(
      from player in Player, where: player.id == ^player_id and player.user_id == ^user_id
    )
  end

  def can_manage_deck?(_user, _deck), do: false

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

  @doc """
  Resolves a player without allowing a display name to override an explicit Discord identity.

  Discord identities match only by `discord_id`; when no identity matches, a distinct player
  name is chosen. Name matching is used only when `discord_id` is absent.
  """
  def resolve_player(name, discord_id, opts \\ []), do: ResolvePlayer.run(name, discord_id, opts)

  def preview_player_resolutions(identities), do: ResolvePlayer.preview(identities)

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
    resolve_player(name, discord_id)
  end

  @doc """
  Folds `source` into `target`: every seat and deck moves to `target`, the
  account/Discord identity is carried over, and `source` is deleted.

  Decks with the same name (case-insensitive) collapse into `target`'s deck.
  Fails with a changeset error when the two players sat in the same game or
  belong to different accounts, since neither can be represented afterwards.
  """
  def merge_players(%Player{} = source, %Player{} = target),
    do: MergePlayers.run(source, target)

  @doc """
  Makes `player` the account's player. The account's current player, if any and
  different, is merged into `player` so no game history is lost.
  """
  def link_player_to_user(%Player{} = player, %User{} = user),
    do: MergePlayers.link_to_user(player, user)

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
    do: deck |> Deck.update_changeset(attrs) |> Repo.update() |> preload_ok(:player)

  def delete_deck(%Deck{} = deck), do: Repo.delete(deck)

  def pick_deck(%User{} = user, opts \\ []), do: DeckPicker.random_deck(user, opts)

  def record_deck_outcome(%User{} = user, deck_id, outcome),
    do: DeckPicker.record_outcome(user, deck_id, outcome)

  def sync_manavault_decks(%User{} = user), do: ManaVaultSync.run(user)

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

  def can_manage_game?(%User{role: "admin"}, %Game{}), do: true
  def can_manage_game?(%User{id: id}, %Game{created_by_user_id: id}), do: true

  def can_manage_game?(%User{id: user_id}, %Game{id: game_id}) do
    Repo.exists?(
      from seat in GamePlayer,
        join: player in Player,
        on: player.id == seat.player_id,
        where: seat.game_id == ^game_id and player.user_id == ^user_id
    )
  end

  def can_manage_game?(_user, _game), do: false

  def create_game(attrs, created_by_user_id \\ nil),
    do: RecordGame.create(attrs, created_by_user_id)

  def find_or_create_game_by_external_id(source, external_id, attrs),
    do: RecordGame.find_or_create_by_external_id(source, external_id, attrs)

  def upsert_game_by_external_id(source, external_id, attrs),
    do: RecordGame.upsert_by_external_id(source, external_id, attrs)

  def update_game(%Game{} = game, attrs), do: RecordGame.update(game, attrs)

  def delete_game(%Game{} = game), do: Repo.delete(game)

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
