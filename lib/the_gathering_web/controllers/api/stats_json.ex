defmodule TheGatheringWeb.API.StatsJSON do
  def show(%{stats: stats}), do: %{data: stats}
end
