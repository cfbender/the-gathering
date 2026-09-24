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

# Credential limits. Password sudo uses a tighter per-account-and-client limit
# plus a global budget so distributed attempts cannot create unbounded bcrypt work.
# Runtime may set `trust_proxy_headers`.
config :the_gathering, TheGatheringWeb.RateLimit,
  credentials: [limit: 10, scale: :timer.minutes(5)],
  corrections: [limit: 30, scale: :timer.minutes(1)],
  sudo: [limit: 5, global_limit: 100, scale: :timer.minutes(5)],
  # Webcam table channels (see TheGatheringWeb.ChannelRateLimit). Every life
  # tap is one event and one SQLite write, so allow bursts of rapid clicking
  # while capping the sustained rate. Signals burst when a seat connects to
  # every peer at once (offer, answer and a dozen ICE candidates per peer).
  webcam_table_events: [capacity: 60, refill_per_second: 20],
  webcam_table_signals: [capacity: 300, refill_per_second: 50],
  webcam_table_joins: [limit: 30, scale: :timer.minutes(1)]

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
    "image",
    "token",
    "secret",
    "manavault_api_key",
    "code",
    "state"
  ]

# Import environment specific config. This must remain at the bottom
# of this file so it overrides the configuration defined above.
import_config "#{config_env()}.exs"
