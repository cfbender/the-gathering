# This file is responsible for configuring your application
# and its dependencies with the aid of the Config module.
#
# This configuration file is loaded before any dependency and
# is restricted to this project.

# General application configuration
import Config

config :the_gathering,
  ecto_repos: [TheGathering.Repo],
  generators: [timestamp_type: :utc_datetime]

# SQLite allows one writer at a time. Deferred transactions that read before
# writing fail immediately with "Database busy" when another connection commits
# in between (SQLITE_BUSY_SNAPSHOT), which is what happens when the catalog sync
# runs while a user registers. Immediate transactions take the write lock up
# front and wait up to busy_timeout instead.
config :the_gathering, TheGathering.Repo,
  default_transaction_mode: :immediate,
  busy_timeout: 5_000

# Per-client limits for the public credential endpoints (admin password login
# and bootstrap registration). Runtime may set `trust_proxy_headers`.
config :the_gathering, TheGatheringWeb.RateLimit,
  credentials: [limit: 10, scale: :timer.minutes(5)]

# Configure the endpoint
config :the_gathering, TheGatheringWeb.Endpoint,
  url: [host: "localhost"],
  adapter: Bandit.PhoenixAdapter,
  render_errors: [
    formats: [json: TheGatheringWeb.ErrorJSON],
    layout: false
  ],
  pubsub_server: TheGathering.PubSub,
  live_view: [signing_salt: "ppgzQhPY"]

# Configure Elixir's Logger
config :logger, :default_formatter,
  format: "$time $metadata[$level] $message\n",
  metadata: [:request_id]

# Use Jason for JSON parsing in Phoenix
config :phoenix,
  json_library: Jason,
  filter_parameters: [
    "password",
    "token",
    "secret",
    "manavault_api_key",
    "code",
    "state"
  ]

# Import environment specific config. This must remain at the bottom
# of this file so it overrides the configuration defined above.
import_config "#{config_env()}.exs"
