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

# Print only warnings and errors during test
config :logger, level: :warning

# Tests invoke catalog sync explicitly with local fixtures.
config :the_gathering, :catalog_sync_enabled, false

# Deck-list tests exercise ManaVault links against a stubbed self-hosted origin.
config :the_gathering, TheGathering.Decklists, manavault_url: "https://manavault.example.com"

# Initialize plugs at runtime for faster test compilation
config :phoenix, :plug_init_mode, :runtime

# Sort query params output of verified routes for robust url comparisons
config :phoenix,
  sort_verified_routes_query_params: true
