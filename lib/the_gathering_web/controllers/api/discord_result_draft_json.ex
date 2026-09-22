defmodule TheGatheringWeb.API.DiscordResultDraftJSON do
  def show(%{draft: draft}), do: %{data: draft}
end
