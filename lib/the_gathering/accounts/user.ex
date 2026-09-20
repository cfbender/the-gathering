defmodule TheGathering.Accounts.User do
  use Ecto.Schema
  import Ecto.Changeset

  @roles ~w(admin member)

  schema "users" do
    field :username, :string
    field :display_name, :string
    field :hashed_password, :string, redact: true
    field :password, :string, virtual: true, redact: true
    field :discord_id, :string
    field :avatar_url, :string
    field :role, :string, default: "member"
    field :disabled_at, :utc_datetime
    field :authenticated_at, :utc_datetime, virtual: true

    timestamps(type: :utc_datetime)
  end

  def registration_changeset(user, attrs, role) do
    user
    |> cast(attrs, [:username, :display_name, :password])
    |> normalize_username()
    |> default_display_name()
    |> put_change(:role, role)
    |> validate_account_fields()
    |> validate_password()
    |> unique_constraint(:username)
  end

  def admin_changeset(user, attrs) do
    user
    |> cast(attrs, [:username, :display_name, :password, :role])
    |> normalize_username()
    |> default_display_name()
    |> validate_account_fields()
    |> validate_password()
    |> unique_constraint(:username)
  end

  def discord_changeset(user, attrs) do
    user
    |> cast(attrs, [:username, :display_name, :discord_id, :avatar_url])
    |> normalize_username()
    |> default_display_name()
    |> put_change(:role, "member")
    |> validate_account_fields()
    |> validate_required([:discord_id])
    |> unique_constraint(:username)
    |> unique_constraint(:discord_id)
  end

  def discord_profile_changeset(user, attrs) do
    user
    |> cast(attrs, [:avatar_url])
  end

  def profile_changeset(user, attrs) do
    user
    |> cast(attrs, [:display_name])
    |> validate_required([:display_name])
    |> validate_length(:display_name, min: 1, max: 80)
  end

  def password_changeset(user, attrs, opts \\ []) do
    user
    |> cast(attrs, [:password])
    |> validate_confirmation(:password, message: "does not match password")
    |> validate_password(opts)
  end

  def admin_update_changeset(user, attrs) do
    user
    |> cast(attrs, [:username, :display_name, :role, :disabled_at])
    |> normalize_username()
    |> validate_account_fields()
    |> unique_constraint(:username)
  end

  defp validate_account_fields(changeset) do
    changeset
    |> validate_required([:username, :display_name, :role])
    |> validate_length(:username, min: 3, max: 40)
    |> validate_format(:username, ~r/^[a-z0-9][a-z0-9_.-]*$/,
      message: "may only contain letters, numbers, dots, dashes, and underscores"
    )
    |> validate_length(:display_name, min: 1, max: 80)
    |> validate_inclusion(:role, @roles)
  end

  def valid_password?(%__MODULE__{hashed_password: hashed_password}, password)
      when is_binary(hashed_password) and is_binary(password) and byte_size(password) > 0 do
    Bcrypt.verify_pass(password, hashed_password)
  end

  def valid_password?(_user, _password) do
    Bcrypt.no_user_verify()
    false
  end

  defp validate_password(changeset, opts \\ []) do
    changeset
    |> validate_required([:password])
    |> validate_length(:password, min: 12, max: 72)
    |> maybe_hash_password(opts)
  end

  defp maybe_hash_password(changeset, opts) do
    password = get_change(changeset, :password)

    if Keyword.get(opts, :hash_password, true) && password && changeset.valid? do
      changeset
      |> validate_length(:password, max: 72, count: :bytes)
      |> put_change(:hashed_password, Bcrypt.hash_pwd_salt(password))
      |> delete_change(:password)
    else
      changeset
    end
  end

  defp normalize_username(changeset) do
    update_change(changeset, :username, &(&1 |> String.trim() |> String.downcase()))
  end

  defp default_display_name(changeset) do
    case get_field(changeset, :display_name) do
      value when value in [nil, ""] ->
        put_change(changeset, :display_name, get_field(changeset, :username))

      _value ->
        update_change(changeset, :display_name, &String.trim/1)
    end
  end
end
