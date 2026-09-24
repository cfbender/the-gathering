defmodule TheGatheringWeb.API.AdminPlayerJSON do
  def index(%{players: players, meta: meta}) do
    %{data: Enum.map(players, &identity/1), meta: meta}
  end

  defp identity(player) do
    %{
      id: player.id,
      name: player.name,
      discord_id: player.discord_id,
      archived_at: player.archived_at,
      user: if(player.user, do: %{id: player.user.id, username: player.user.username})
    }
  end
end
