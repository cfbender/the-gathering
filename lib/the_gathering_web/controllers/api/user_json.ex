defmodule TheGatheringWeb.API.UserJSON do
  alias TheGathering.Accounts.User

  def index(%{users: users}), do: %{data: Enum.map(users, &data/1)}
  def show(%{user: user}), do: %{data: data(user)}

  def data(%User{} = user) do
    %{
      id: user.id,
      username: user.username,
      display_name: user.display_name,
      discord_id: user.discord_id,
      avatar_url: user.avatar_url,
      moxfield_username: user.moxfield_username,
      archidekt_username: user.archidekt_username,
      manavault_url: user.manavault_url,
      has_manavault_api_key: not is_nil(user.manavault_api_key),
      has_password: not is_nil(user.hashed_password),
      palette: user.palette,
      theme_style: user.theme_style,
      role: user.role,
      disabled: not is_nil(user.disabled_at),
      inserted_at: user.inserted_at
    }
  end
end
