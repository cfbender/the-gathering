defmodule TheGathering.WebcamTables.Pruner do
  @moduledoc """
  Cleans up webcam tables every minute: closes rooms that have had no
  connections and no activity for 30 minutes (deleting their sessions), then
  deletes expired sessions. Running rooms refresh their own expiry, so only
  abandoned tables are removed.
  """
  use GenServer

  alias TheGathering.WebcamTables
  alias TheGathering.WebcamTables.Session

  @interval :timer.minutes(1)
  @idle_timeout :timer.minutes(30)

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @impl true
  def init(_opts) do
    if Application.get_env(:the_gathering, :webcam_table_pruning_enabled, true), do: schedule()
    {:ok, nil}
  end

  @impl true
  def handle_info(:prune, state) do
    WebcamTables.close_idle_rooms(@idle_timeout)
    Session.prune()
    schedule()
    {:noreply, state}
  end

  defp schedule, do: Process.send_after(self(), :prune, @interval)
end
