defmodule TheGathering.Accounts do
  @moduledoc "User accounts, authentication, and server registration settings."

  import Ecto.Query

  alias Ecto.Multi
  alias TheGathering.Accounts.{ServerSettings, User, UserToken}
  alias TheGathering.Repo

  def registration_status do
    user_count = Repo.aggregate(User, :count)

    %{
      allowed: user_count == 0 or get_settings().registration_enabled,
      bootstrap: user_count == 0
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

  def get_user_by_username_and_password(username, password)
      when is_binary(username) and is_binary(password) do
    user = Repo.get_by(User, username: username |> String.trim() |> String.downcase())

    if user && is_nil(user.disabled_at) && User.valid_password?(user, password),
      do: user,
      else: invalid_password(user, password)
  end

  def get_user_by_username_and_password(_username, _password) do
    Bcrypt.no_user_verify()
    nil
  end

  def get_user(id), do: Repo.get(User, id)
  def get_user_by_username(username), do: Repo.get_by(User, username: String.downcase(username))
  def list_users, do: Repo.all(from u in User, order_by: [asc: u.username])

  def update_profile(user, attrs), do: user |> User.profile_changeset(attrs) |> Repo.update()

  def sudo_mode?(user, minutes \\ -20)

  def sudo_mode?(%User{authenticated_at: authenticated_at}, minutes)
      when is_struct(authenticated_at, DateTime) do
    DateTime.after?(authenticated_at, DateTime.utc_now() |> DateTime.add(minutes, :minute))
  end

  def sudo_mode?(_user, _minutes), do: false

  def update_user_password(user, attrs) do
    user
    |> User.password_changeset(attrs)
    |> update_user_and_delete_all_tokens()
  end

  def reset_password(user, attrs), do: update_user_password(user, attrs)

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

  defp register_when_allowed(%{allowed: false}, _attrs), do: Repo.rollback(:registration_closed)

  defp register_when_allowed(status, attrs) do
    role = if status.bootstrap, do: "admin", else: "member"

    case %User{} |> User.registration_changeset(attrs, role) |> Repo.insert() do
      {:ok, user} -> user
      {:error, changeset} -> Repo.rollback(changeset)
    end
  end

  defp invalid_password(user, password) do
    unless user && is_nil(user.disabled_at), do: User.valid_password?(nil, password)
    nil
  end

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
