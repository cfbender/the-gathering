import Config

# config/runtime.exs is executed for all environments, including
# during releases. It is executed after compilation and before the
# system starts, so it is typically used to load production configuration
# and secrets from environment variables or elsewhere. Do not define
# any compile-time configuration in here, as it won't be applied.

# The container image sets PHX_SERVER=true. Local `mix phx.server` sets it too.
if System.get_env("PHX_SERVER") do
  config :the_gathering, TheGatheringWeb.Endpoint, server: true
end

config :the_gathering, TheGatheringWeb.Endpoint,
  http: [port: String.to_integer(System.get_env("PORT", "4000"))]

catalog_sync_hours = String.to_integer(System.get_env("CATALOG_SYNC_INTERVAL_HOURS", "168"))
config :the_gathering, :catalog_sync_interval_ms, catalog_sync_hours * 60 * 60 * 1_000

if discord_bot_token = System.get_env("DISCORD_BOT_TOKEN") do
  config :nostrum,
    token: discord_bot_token,
    gateway_intents: [:guilds, :guild_messages, :message_content],
    ffmpeg: false,
    youtubedl: false,
    streamlink: false

  config :the_gathering, TheGathering.Discord,
    bot_token: discord_bot_token,
    guild_id: System.get_env("DISCORD_GUILD_ID"),
    spellbot_user_id: System.get_env("DISCORD_SPELLBOT_USER_ID", "725510263251402832")
end

if config_env() == :prod do
  data_dir = System.get_env("DATA_DIR", "/data")
  database_path = System.get_env("DATABASE_PATH", Path.join(data_dir, "the_gathering.db"))

  config :the_gathering, :data_dir, data_dir

  config :the_gathering, TheGathering.Repo,
    database: database_path,
    pool_size: String.to_integer(System.get_env("POOL_SIZE") || "5")

  # The secret key base is used to sign/encrypt cookies and other secrets.
  secret_key_base =
    System.get_env("SECRET_KEY_BASE") ||
      raise """
      environment variable SECRET_KEY_BASE is missing.
      You can generate one by calling: mix phx.gen.secret
      """

  # PHX_HOST is the public hostname users reach the app at, e.g. games.example.com.
  host = System.get_env("PHX_HOST", "localhost")
  scheme = System.get_env("PHX_SCHEME", "https")

  url_port =
    String.to_integer(
      System.get_env("PHX_URL_PORT", if(scheme == "https", do: "443", else: "80"))
    )

  config :the_gathering, :dns_cluster_query, System.get_env("DNS_CLUSTER_QUERY")

  config :the_gathering, TheGatheringWeb.Endpoint,
    url: [host: host, port: url_port, scheme: scheme],
    http: [
      # Bind on all interfaces (IPv4 and IPv6) so the container port mapping works.
      ip: {0, 0, 0, 0, 0, 0, 0, 0}
    ],
    secret_key_base: secret_key_base
end
