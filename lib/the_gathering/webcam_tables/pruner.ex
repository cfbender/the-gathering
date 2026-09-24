defmodule TheGathering.WebcamTables.Pruner do
  @moduledoc """
  Deletes expired webcam table sessions every hour. Running rooms refresh
  their own expiry, so only abandoned tables are removed.
  """
  use GenServer

  alias TheGathering.WebcamTables.Session

  @interval :timer.hours(1)

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  @impl true
  def init(_opts) do
    schedule()
    {:ok, nil}
  end

  @impl true
  def handle_info(:prune, state) do
    Session.prune()
    schedule()
    {:noreply, state}
  end

  defp schedule, do: Process.send_after(self(), :prune, @interval)
end
