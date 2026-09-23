defmodule TheGathering.Catalog.Supervisor do
  @moduledoc false

  use Supervisor

  def start_link(opts), do: Supervisor.start_link(__MODULE__, opts, name: __MODULE__)

  @impl true
  def init(_opts) do
    Supervisor.init(
      [
        {Task.Supervisor, name: TheGathering.Catalog.TaskSupervisor},
        TheGathering.Catalog.CardImages,
        TheGathering.Catalog.SyncServer
      ],
      strategy: :one_for_one
    )
  end
end
