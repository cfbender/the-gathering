defmodule TheGatheringWeb.API.AdminRegistrationInviteJSON do
  def show(%{enabled: enabled}), do: %{data: %{enabled: enabled}}
  def create(%{token: token}), do: %{data: %{token: token}}
end
