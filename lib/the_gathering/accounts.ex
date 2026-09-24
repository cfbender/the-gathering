defmodule TheGathering.Accounts do
  @moduledoc "User accounts, authentication, and server registration settings."

  import Ecto.Query

  alias Ecto.Multi

  alias TheGathering.Accounts.{
    RegistrationInvite,
    ServerSettings,
    SignInWithDiscord,
    User,
    UserToken
  }

  alias TheGathering.Games.{Deck, GamePlayer, Player}
  alias TheGathering.Repo

  def registration_status do
    user_count = Repo.aggregate(User, :count)

    %{
      allowed: user_count == 0 or get_settings().registration_enabled,
      bootstrap: user_count == 0,
      discord_configured: TheGathering.DiscordOAuth.configured?()
    }
  end

  def register_user(attrs) do
    Repo.transaction(fn -> register_when_allowed(registration_status(), attrs) end)
  end

  def create_user(attrs) do
    %User{}
    |> User.admin_changeset(attrs)
    |> Repo.insert()
  end

  def create_admin(attrs), do: attrs |> Map.put("role", "admin") |> create_user()

  @doc """
  Returns the first enabled administrator, creating a passwordless `dev` administrator
  when none exists. Used only by the development auto sign-in (`:dev_auto_login`).
  """
  def get_or_create_dev_admin do
    query =
      from u in User,
        where: u.role == "admin" and is_nil(u.disabled_at),
        order_by: [asc: u.id],
        limit: 1

    Repo.one(query) ||
      Repo.insert!(%User{username: "dev", display_name: "Developer", role: "admin"})
  end

  def get_user_by_username_and_password(username, password)
      when is_binary(username) and is_binary(password) do
    user = Repo.get_by(User, username: username |> String.trim() |> String.downcase())

    if user && user.role == "admin" && is_nil(user.disabled_at) &&
         User.valid_password?(user, password),
       do: user,
       else: invalid_password(user, password)
  end

  def get_user_by_username_and_password(_username, _password) do
    Bcrypt.no_user_verify()
    nil
  end

  def get_user(id), do: Repo.get(User, id)
  def get_user_by_discord_id(discord_id), do: Repo.get_by(User, discord_id: discord_id)
  def get_user_by_username(username), do: Repo.get_by(User, username: String.downcase(username))
  def list_users, do: Repo.all(from u in User, order_by: [asc: u.username])

  def update_profile(user, attrs) do
    changeset = User.profile_changeset(user, attrs)

    Multi.new()
    |> Multi.update(:user, changeset)
    |> rename_linked_player(changeset)
    |> Repo.transaction()
    |> case do
      {:ok, %{user: updated}} -> {:ok, updated}
      {:error, _operation, failed_changeset, _changes} -> {:error, failed_changeset}
    end
  end

  def sudo_mode?(user, minutes \\ -20)

  def sudo_mode?(%User{authenticated_at: authenticated_at}, minutes)
      when is_struct(authenticated_at, DateTime) do
    DateTime.after?(authenticated_at, DateTime.utc_now() |> DateTime.add(minutes, :minute))
  end

  def sudo_mode?(_user, _minutes), do: false

  def update_user_password(%User{role: "admin", hashed_password: hashed_password} = user, attrs)
      when not is_nil(hashed_password) do
    user
    |> User.password_changeset(attrs)
    |> update_user_and_delete_all_tokens()
  end

  def update_user_password(_user, _attrs), do: {:error, :forbidden}

  def generate_user_session_token(user) do
    prune_expired_user_session_tokens()
    {token, user_token} = UserToken.build_session_token(user)
    Repo.insert!(user_token)
    token
  end

  def prune_expired_user_session_tokens do
    {count, _tokens} = Repo.delete_all(UserToken.expired_session_tokens_query())
    count
  end

  def get_user_by_session_token(token) do
    {:ok, query} = UserToken.verify_session_token_query(token)
    Repo.one(query)
  end

  def delete_user_session_token(token) do
    Repo.delete_all(from UserToken, where: [token: ^token, context: "session"])
    :ok
  end

  def revoke_all_sessions(%User{} = user) do
    Repo.delete_all(all_user_tokens_query(user))
    {:ok, user}
  end

  def update_user(user, attrs) do
    changeset = User.admin_update_changeset(user, attrs)

    Multi.new()
    |> Multi.run(:last_admin, fn repo, _changes -> ensure_enabled_admin(repo, user, changeset) end)
    |> Multi.update(:user, changeset)
    |> rename_linked_player(changeset)
    |> maybe_revoke_disabled_sessions(user, changeset)
    |> Repo.transaction()
    |> case do
      {:ok, %{user: updated}} -> {:ok, updated}
      {:error, _operation, failed_changeset, _changes} -> {:error, failed_changeset}
    end
  end

  def disable_user(user) do
    update_user(user, %{disabled_at: DateTime.utc_now() |> DateTime.truncate(:second)})
  end

  def delete_user(%User{} = user, %User{} = actor) do
    players = from player in Player, where: player.user_id == ^user.id
    player_ids = from player in players, select: player.id
    decks = from deck in Deck, where: deck.player_id in subquery(player_ids)
    deck_ids = from deck in decks, select: deck.id

    Multi.new()
    |> Multi.run(:authorization, fn repo, _changes ->
      authorize_user_deletion(repo, user, actor)
    end)
    |> Multi.run(:game_history, fn repo, _changes ->
      references =
        from seat in GamePlayer,
          where:
            seat.player_id in subquery(player_ids) or
              seat.eliminated_by_player_id in subquery(player_ids) or
              seat.deck_id in subquery(deck_ids)

      if repo.exists?(references) do
        {:error,
         user
         |> Ecto.Changeset.change()
         |> Ecto.Changeset.add_error(:player, "must have zero games before deleting this user")}
      else
        {:ok, :empty}
      end
    end)
    |> Multi.delete_all(:decks, decks)
    |> Multi.delete_all(:players, players)
    |> Multi.update_all(
      :games,
      from(game in TheGathering.Games.Game, where: game.created_by_user_id == ^user.id),
      set: [created_by_user_id: nil]
    )
    |> Multi.delete_all(:tokens, all_user_tokens_query(user))
    |> Multi.delete(:user, user)
    |> Repo.transaction()
    |> case do
      {:ok, %{user: deleted}} -> {:ok, deleted}
      {:error, :authorization, :forbidden, _changes} -> {:error, :forbidden}
      {:error, _operation, reason, _changes} -> {:error, reason}
    end
  end

  def get_settings, do: Repo.get!(ServerSettings, 1)

  def update_settings(attrs) do
    get_settings()
    |> ServerSettings.changeset(attrs)
    |> Repo.update()
  end

  defdelegate rotate_registration_invite(), to: RegistrationInvite, as: :rotate
  defdelegate registration_invite_hash(token), to: RegistrationInvite, as: :hash
  defdelegate valid_registration_invite_hash?(hash), to: RegistrationInvite, as: :valid_hash?

  def sign_in_with_discord(claims, invite_hash \\ nil),
    do: SignInWithDiscord.run(claims, invite_hash)

  defp register_when_allowed(%{allowed: false}, _attrs), do: Repo.rollback(:registration_closed)
  defp register_when_allowed(%{bootstrap: false}, _attrs), do: Repo.rollback(:registration_closed)

  defp register_when_allowed(_status, attrs) do
    case %User{} |> User.registration_changeset(attrs, "admin") |> Repo.insert() do
      {:ok, user} -> user
      {:error, changeset} -> Repo.rollback(changeset)
    end
  end

  defp invalid_password(user, password) do
    unless user && user.role == "admin" && is_nil(user.disabled_at),
      do: User.valid_password?(nil, password)

    nil
  end

  defp update_user_and_delete_all_tokens(changeset) do
    Multi.new()
    |> Multi.update(:user, changeset)
    |> Multi.run(:tokens, fn repo, %{user: user} ->
      tokens = repo.all_by(UserToken, user_id: user.id)
      repo.delete_all(all_user_tokens_query(user))
      {:ok, tokens}
    end)
    |> Repo.transaction()
    |> case do
      {:ok, %{user: user, tokens: tokens}} -> {:ok, {user, tokens}}
      {:error, :user, changeset, _changes} -> {:error, changeset}
    end
  end

  defp all_user_tokens_query(user) do
    from token in UserToken, where: token.user_id == ^user.id
  end

  defp authorize_user_deletion(_repo, %User{id: id}, %User{id: id}), do: {:error, :forbidden}

  defp authorize_user_deletion(repo, %User{role: "admin", id: id}, _actor) do
    if repo.exists?(from user in User, where: user.id != ^id and user.role == "admin"),
      do: {:ok, :authorized},
      else: {:error, :forbidden}
  end

  defp authorize_user_deletion(_repo, _user, _actor), do: {:ok, :authorized}

  # Games, stats, and Discord show a player's `name`, so a new display name has to
  # reach the linked player or the edit is invisible outside the account menu.
  defp rename_linked_player(multi, user_changeset) do
    case Ecto.Changeset.get_change(user_changeset, :display_name) do
      nil ->
        multi

      display_name ->
        Multi.run(multi, :player, fn repo, %{user: user} ->
          repo
          |> rename_player(repo.get_by(Player, user_id: user.id), display_name)
          |> player_name_result(user_changeset)
        end)
    end
  end

  defp rename_player(_repo, nil, _name), do: {:ok, nil}

  defp rename_player(repo, %Player{} = player, name),
    do: player |> Player.changeset(%{name: name}) |> repo.update()

  defp player_name_result({:ok, player}, _user_changeset), do: {:ok, player}

  defp player_name_result({:error, _player_changeset}, user_changeset),
    do:
      {:error,
       Ecto.Changeset.add_error(
         user_changeset,
         :display_name,
         "is already used by another player"
       )}

  defp maybe_revoke_disabled_sessions(multi, user, changeset) do
    becoming_disabled? =
      is_nil(user.disabled_at) and
        not is_nil(Ecto.Changeset.get_field(changeset, :disabled_at))

    if becoming_disabled? do
      Multi.delete_all(
        multi,
        :tokens,
        from(token in UserToken, where: token.user_id == ^user.id and token.context == "session")
      )
    else
      multi
    end
  end

  defp ensure_enabled_admin(repo, user, changeset) do
    becoming_inactive? =
      user.role == "admin" and is_nil(user.disabled_at) and
        (Ecto.Changeset.get_field(changeset, :role) != "admin" or
           not is_nil(Ecto.Changeset.get_field(changeset, :disabled_at)))

    other_admin_exists? =
      repo.exists?(
        from u in User,
          where: u.id != ^user.id and u.role == "admin" and is_nil(u.disabled_at)
      )

    if becoming_inactive? and not other_admin_exists? do
      {:error,
       Ecto.Changeset.add_error(changeset, :role, "must leave at least one enabled admin")}
    else
      {:ok, :valid}
    end
  end
end
