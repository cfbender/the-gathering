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
    field :moxfield_username, :string
    field :archidekt_username, :string
    field :manavault_url, :string
    field :manavault_api_key, TheGathering.Accounts.EncryptedString, redact: true
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

  @doc """
  Updates the user's own profile and deck-host settings.

  `manavault_api_key` is write-only: a blank value keeps the stored key, an
  explicit `nil` removes it, and any other string replaces it.
  """
  def profile_changeset(user, attrs) do
    user
    |> cast(keep_blank_api_key(attrs), [
      :display_name,
      :moxfield_username,
      :archidekt_username,
      :manavault_url,
      :manavault_api_key
    ])
    |> normalize_deck_sources()
    |> validate_required([:display_name])
    |> validate_length(:display_name, min: 1, max: 80)
    |> validate_length(:moxfield_username, max: 80)
    |> validate_length(:archidekt_username, max: 80)
    |> validate_length(:manavault_url, max: 2_048)
    |> validate_length(:manavault_api_key, max: 512)
    |> validate_format(:moxfield_username, ~r/^[^\s\/]+$/,
      message: "must be a username, not a URL"
    )
    |> validate_format(:archidekt_username, ~r/^[^\s\/]+$/,
      message: "must be a username, not a URL"
    )
    |> validate_change(:manavault_url, &validate_http_url/2)
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

  defp normalize_deck_sources(changeset) do
    changeset
    |> update_change(:moxfield_username, &trim/1)
    |> update_change(:archidekt_username, &trim/1)
    |> update_change(:manavault_url, &(&1 |> trim() |> String.trim_trailing("/")))
    |> update_change(:manavault_api_key, &trim/1)
  end

  defp trim(nil), do: nil
  defp trim(value), do: String.trim(value)

  # A blank key means "leave the stored key alone"; only an explicit nil clears it.
  defp keep_blank_api_key(attrs) when is_map(attrs) do
    Enum.reduce(["manavault_api_key", :manavault_api_key], attrs, fn key, attrs ->
      case Map.fetch(attrs, key) do
        {:ok, value} when is_binary(value) -> drop_if_blank(attrs, key, value)
        _ -> attrs
      end
    end)
  end

  defp keep_blank_api_key(attrs), do: attrs

  defp drop_if_blank(attrs, key, value) do
    if String.trim(value) == "", do: Map.delete(attrs, key), else: attrs
  end

  defp validate_http_url(field, value) do
    case URI.parse(value) do
      %URI{scheme: scheme, host: host} when scheme in ["http", "https"] and is_binary(host) -> []
      _ -> [{field, "must be a valid http(s) URL"}]
    end
  end
end
