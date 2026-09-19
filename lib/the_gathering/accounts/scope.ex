defmodule TheGathering.Accounts.Scope do
  @moduledoc "Carries the authenticated caller through application boundaries."

  alias TheGathering.Accounts.User

  defstruct user: nil

  def for_user(%User{} = user), do: %__MODULE__{user: user}
  def for_user(nil), do: %__MODULE__{user: nil}
end
