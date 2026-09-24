defmodule TheGathering.Application do
  # See https://elixir.hexdocs.pm/Application.html
  # for more information on OTP Applications
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      TheGatheringWeb.Telemetry,
      TheGathering.Repo,
      {Ecto.Migrator,
       repos: Application.fetch_env!(:the_gathering, :ecto_repos), skip: skip_migrations?()},
      {DNSCluster, query: Application.get_env(:the_gathering, :dns_cluster_query) || :ignore},
      {Phoenix.PubSub, name: TheGathering.PubSub},
      TheGatheringWeb.Presence,
      {Registry, keys: :unique, name: TheGathering.WebcamTables.Registry},
      {DynamicSupervisor, name: TheGathering.WebcamTables.RoomSupervisor, strategy: :one_for_one},
      TheGathering.WebcamTables.Pruner,
      {TheGathering.RateLimiter, clean_period: :timer.minutes(10)},
      TheGathering.Catalog.Supervisor,
      TheGathering.Decklists.Cache,
      # Start a worker by calling: TheGathering.Worker.start_link(arg)
      # {TheGathering.Worker, arg},
      TheGatheringWeb.Endpoint,
      # Queue recovery may generate public table URLs as soon as Discord starts.
      TheGathering.Discord
    ]

    # See https://elixir.hexdocs.pm/Supervisor.html
    # for other strategies and supported options
    opts = [strategy: :one_for_one, name: TheGathering.Supervisor]

    with {:ok, pid} <- Supervisor.start_link(children, opts) do
      # After the Endpoint is up so the logged redirect URI reflects PHX_* settings.
      TheGathering.DiscordOAuth.log_status()
      {:ok, pid}
    end
  end

  # Tell Phoenix to update the endpoint configuration
  # whenever the application is updated.
  @impl true
  def config_change(changed, _new, removed) do
    TheGatheringWeb.Endpoint.config_change(changed, removed)
    :ok
  end

  defp skip_migrations? do
    # By default, sqlite migrations are run when using a release
    System.get_env("RELEASE_NAME") == nil
  end
end
