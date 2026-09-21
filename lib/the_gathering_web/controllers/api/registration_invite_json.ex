defmodule TheGatheringWeb.API.RegistrationInviteJSON do
  def show(%{valid: valid}), do: %{data: %{valid: valid}}
end
