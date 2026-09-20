defmodule TheGathering.Accounts.EncryptedString do
  @moduledoc """
  An Ecto type that stores a string encrypted with the endpoint `secret_key_base`.

  Used for third-party credentials (such as ManaVault API keys) so a copied
  database file does not leak them. Rotating `SECRET_KEY_BASE` invalidates the
  stored values, which then load as `nil`.
  """

  use Ecto.Type

  @salt "the_gathering.accounts.encrypted_string"

  @impl true
  def type, do: :string

  @impl true
  def cast(value) when is_binary(value), do: {:ok, value}
  def cast(nil), do: {:ok, nil}
  def cast(_value), do: :error

  @impl true
  def dump(value) when is_binary(value), do: {:ok, Plug.Crypto.encrypt(secret(), @salt, value)}
  def dump(nil), do: {:ok, nil}
  def dump(_value), do: :error

  @impl true
  def load(value) when is_binary(value) do
    case Plug.Crypto.decrypt(secret(), @salt, value, max_age: :infinity) do
      {:ok, plain} when is_binary(plain) -> {:ok, plain}
      _ -> {:ok, nil}
    end
  end

  def load(nil), do: {:ok, nil}
  def load(_value), do: :error

  defp secret do
    Application.fetch_env!(:the_gathering, TheGatheringWeb.Endpoint)[:secret_key_base]
  end
end
