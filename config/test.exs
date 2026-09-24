import Config

# Configure your database
#
# The MIX_TEST_PARTITION environment variable can be used
# to provide built-in test partitioning in CI environment.
# Run `mix help test` for more information.
config :the_gathering, TheGathering.Repo,
  database: Path.expand("../the_gathering_test.db", __DIR__),
  pool_size: 5,
  pool: Ecto.Adapters.SQL.Sandbox

# We don't run a server during test. If one is required,
# you can enable the server option below.
config :the_gathering, TheGatheringWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: 4002],
  secret_key_base: "BxVic2xATqUYX7g8UgEVQl/MU+DF57PUsRKqbop07yJKwjbf0bLH69WiPtbXtkHl",
  server: false

# Tests never build the React bundle, so avoid reading the Vite manifest.
config :the_gathering, TheGatheringWeb.ViteAssets, mode: :dev_server

# Tests that need runtime data files (card-recognition bundles) write them here.
config :the_gathering, :data_dir, Path.expand("../tmp/test_data", __DIR__)

# Print only warnings and errors during test
config :logger, level: :warning

# Tests invoke catalog sync explicitly with local fixtures.
config :the_gathering, :catalog_sync_enabled, false

# Every ConnTest request shares 127.0.0.1, so keep the shared bucket effectively
# unlimited; the rate limit tests lower it for their own addresses.
config :the_gathering, TheGatheringWeb.RateLimit,
  credentials: [limit: 1_000_000, scale: :timer.minutes(5)],
  sudo: [limit: 1_000_000, global_limit: 1_000_000, scale: :timer.minutes(5)],
  webcam_table_events: [capacity: 1_000_000, refill_per_second: 1_000_000],
  webcam_table_signals: [capacity: 1_000_000, refill_per_second: 1_000_000],
  webcam_table_joins: [limit: 1_000_000, scale: :timer.minutes(1)]

# Deck-list tests exercise ManaVault links against a stubbed self-hosted origin.
config :the_gathering, TheGathering.Decklists, manavault_url: "https://manavault.example.com"

config :the_gathering, :discord_oauth,
  client_id: "discord-client-id",
  client_secret: "discord-client-secret",
  http_adapter: {Assent.HTTPAdapter.Req, plug: {Req.Test, TheGathering.DiscordOAuth}}

# Initialize plugs at runtime for faster test compilation
config :phoenix, :plug_init_mode, :runtime

# Sort query params output of verified routes for robust url comparisons
config :phoenix,
  sort_verified_routes_query_params: true
