defmodule TheGathering.Accounts.ApiKey do
  @moduledoc """
  A personal API key that authenticates as its owner.

  Only the SHA-256 digest of the secret is stored; the `tg_…` token is revealed
  once, at creation. `prefix` keeps the first characters so owners can tell keys
  apart without the secret.
  """
  use Ecto.Schema
  import Ecto.Changeset

  alias TheGathering.Accounts.User

  @token_prefix "tg_"
  @rand_size 32
  @display_prefix_length 10

  schema "api_keys" do
    field :name, :string
    field :token_hash, :binary, redact: true
    field :prefix, :string
    field :last_used_at, :utc_datetime
    belongs_to :user, User

    timestamps(type: :utc_datetime, updated_at: false)
  end

  @doc "Builds a new key for `user`, returning the one-time token and the changeset to insert."
  def build(%User{id: user_id}, attrs) do
    token =
      @token_prefix <> Base.url_encode64(:crypto.strong_rand_bytes(@rand_size), padding: false)

    changeset =
      %__MODULE__{user_id: user_id}
      |> cast(attrs, [:name])
      |> update_change(:name, &String.trim/1)
      |> validate_required([:name])
      |> validate_length(:name, max: 60)
      |> put_change(:token_hash, hash(token))
      |> put_change(:prefix, String.slice(token, 0, @display_prefix_length))
      |> unique_constraint(:token_hash)

    {token, changeset}
  end

  @doc "Digest used to look a token up; `nil` for values that cannot be API keys."
  def hash(@token_prefix <> _rest = token), do: :crypto.hash(:sha256, token)
  def hash(_token), do: nil
end
