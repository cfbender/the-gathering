defmodule TheGathering.Accounts.RegistrationInvite do
  @moduledoc false

  alias TheGathering.Accounts.ServerSettings
  alias TheGathering.Repo

  # Only the digest is persisted. The reusable secret is revealed once, on rotation.
  def rotate do
    token = :crypto.strong_rand_bytes(32) |> Base.url_encode64(padding: false)

    Repo.get!(ServerSettings, 1)
    |> Ecto.Changeset.change(registration_invite_hash: hash(token))
    |> Repo.update()
    |> case do
      {:ok, _settings} -> {:ok, token}
      {:error, changeset} -> {:error, changeset}
    end
  end

  def hash(token) when is_binary(token) and byte_size(token) == 43,
    do: :crypto.hash(:sha256, token)

  def hash(_token), do: nil

  def valid_hash?(hash) when is_binary(hash) and byte_size(hash) == 32 do
    case Repo.get!(ServerSettings, 1).registration_invite_hash do
      nil -> false
      current -> Plug.Crypto.secure_compare(current, hash)
    end
  end

  def valid_hash?(_hash), do: false
end
