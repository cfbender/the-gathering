defmodule TheGatheringWeb.API.UserJSON do
  alias TheGathering.Accounts.User

  def index(%{users: users}), do: %{data: Enum.map(users, &data/1)}
  def show(%{user: user}), do: %{data: data(user)}

  def data(%User{} = user) do
    %{
      id: user.id,
      username: user.username,
      display_name: user.display_name,
      role: user.role,
      disabled: not is_nil(user.disabled_at),
      inserted_at: user.inserted_at
    }
  end
end
