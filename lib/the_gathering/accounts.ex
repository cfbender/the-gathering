defmodule TheGathering.Accounts do
  @moduledoc "User accounts, authentication, and server registration settings."

  import Ecto.Query

  alias Ecto.Multi
  alias TheGathering.Accounts.{ServerSettings, User, UserToken}
  alias TheGathering.Games
  alias TheGathering.Games.Player
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

  def update_profile(user, attrs), do: user |> User.profile_changeset(attrs) |> Repo.update()

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
    {token, user_token} = UserToken.build_session_token(user)
    Repo.insert!(user_token)
    token
  end

  def get_user_by_session_token(token) do
    {:ok, query} = UserToken.verify_session_token_query(token)
    Repo.one(query)
  end

  def delete_user_session_token(token) do
    Repo.delete_all(from UserToken, where: [token: ^token, context: "session"])
    :ok
  end

  def update_user(user, attrs) do
    changeset = User.admin_update_changeset(user, attrs)

    Multi.new()
    |> Multi.run(:last_admin, fn repo, _changes -> ensure_enabled_admin(repo, user, changeset) end)
    |> Multi.update(:user, changeset)
    |> Repo.transaction()
    |> case do
      {:ok, %{user: updated}} -> {:ok, updated}
      {:error, _operation, failed_changeset, _changes} -> {:error, failed_changeset}
    end
  end

  def disable_user(user) do
    update_user(user, %{disabled_at: DateTime.utc_now() |> DateTime.truncate(:second)})
  end

  def get_settings, do: Repo.get!(ServerSettings, 1)

  def update_settings(attrs) do
    get_settings()
    |> ServerSettings.changeset(attrs)
    |> Repo.update()
  end

  def sign_in_with_discord(%{"sub" => discord_id} = claims) when is_binary(discord_id) do
    Repo.transaction(fn ->
      user =
        case Repo.get_by(User, discord_id: discord_id) do
          %User{disabled_at: disabled_at} when not is_nil(disabled_at) ->
            Repo.rollback(:disabled)

          %User{} = user ->
            user
            |> User.discord_profile_changeset(%{avatar_url: discord_avatar_url(claims)})
            |> Repo.update!()

          nil ->
            create_discord_user(discord_id, claims)
        end

      link_discord_player(user)
      user
    end)
  end

  def sign_in_with_discord(_claims), do: {:error, :invalid_discord_user}

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

  defp create_discord_user(discord_id, claims) do
    status = registration_status()
    unless status.allowed and not status.bootstrap, do: Repo.rollback(:registration_closed)

    username = available_discord_username(claims["preferred_username"], discord_id)

    attrs = %{
      username: username,
      display_name: claims["preferred_username"] || username,
      discord_id: discord_id,
      avatar_url: discord_avatar_url(claims)
    }

    %User{}
    |> User.discord_changeset(attrs)
    |> Repo.insert!()
  end

  defp available_discord_username(preferred_username, discord_id) do
    base =
      preferred_username
      |> to_string()
      |> String.trim()
      |> String.downcase()
      |> String.replace(~r/[^a-z0-9_.-]/, "_")
      |> String.trim("_.-")
      |> String.slice(0, 32)

    base = if String.length(base) >= 3, do: base, else: "discord"

    taken? = fn candidate ->
      Repo.exists?(from user in User, where: user.username == ^candidate)
    end

    first_available(base, &"#{String.slice(base, 0, 38)}#{&1}", taken?) ||
      "#{String.slice(base, 0, 19)}_#{String.slice(discord_id, 0, 20)}"
  end

  # Tries `base`, then `with_suffix.(2)` … `with_suffix.(9)`, so a clash yields
  # a short readable name rather than the Discord snowflake. Returns nil when all
  # are taken so the caller can fall back to the snowflake.
  @suffix_attempts 2..9

  defp first_available(base, with_suffix, taken?) do
    [base | Enum.map(@suffix_attempts, with_suffix)]
    |> Enum.find(fn candidate -> not taken?.(candidate) end)
  end

  defp link_discord_player(user) do
    case Repo.get_by(Player, discord_id: user.discord_id) do
      nil ->
        name = available_player_name(user.display_name, user.discord_id)

        %Player{}
        |> Player.changeset(%{name: name, user_id: user.id, discord_id: user.discord_id})
        |> Repo.insert!()

      %Player{user_id: nil} = player ->
        player |> Player.changeset(%{user_id: user.id}) |> Repo.update!()

      %Player{user_id: user_id} when user_id == user.id ->
        :ok

      _player ->
        Repo.rollback(:discord_identity_conflict)
    end
  end

  defp available_player_name(display_name, discord_id) do
    name = display_name |> String.trim() |> String.slice(0, 76)

    taken? = fn candidate ->
      Repo.exists?(
        from player in Player,
          where: fragment("lower(?)", player.name) == ^Games.fold_name(candidate)
      )
    end

    first_available(name, &"#{name} (#{&1})", taken?) ||
      "#{name} (#{String.slice(discord_id, 0, 20)})"
  end

  defp discord_avatar_url(%{"picture" => picture}) when is_binary(picture) do
    unless String.ends_with?(picture, "/nil"), do: picture
  end

  defp discord_avatar_url(_claims), do: nil

  defp update_user_and_delete_all_tokens(changeset) do
    Multi.new()
    |> Multi.update(:user, changeset)
    |> Multi.run(:tokens, fn repo, %{user: user} ->
      tokens = repo.all_by(UserToken, user_id: user.id)
      repo.delete_all(from token in UserToken, where: token.user_id == ^user.id)
      {:ok, tokens}
    end)
    |> Repo.transaction()
    |> case do
      {:ok, %{user: user, tokens: tokens}} -> {:ok, {user, tokens}}
      {:error, :user, changeset, _changes} -> {:error, changeset}
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
