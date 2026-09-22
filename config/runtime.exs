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

discord_client_id = System.get_env("DISCORD_CLIENT_ID")
discord_client_secret = System.get_env("DISCORD_CLIENT_SECRET")

case {discord_client_id not in [nil, ""], discord_client_secret not in [nil, ""]} do
  {true, true} ->
    config :the_gathering, :discord_oauth,
      client_id: discord_client_id,
      client_secret: discord_client_secret

  {false, false} ->
    :ok

  _one_of_two ->
    IO.warn(
      "Discord OAuth sign-in stays disabled: set both DISCORD_CLIENT_ID and DISCORD_CLIENT_SECRET",
      []
    )
end

# Behind a reverse proxy, identify clients for rate limiting by the proxy's
# x-real-ip / x-forwarded-for headers instead of the proxy address.
config :the_gathering, TheGatheringWeb.RateLimit,
  trust_proxy_headers: System.get_env("TRUST_PROXY_HEADERS") in ["true", "1"]

catalog_sync_hours = String.to_integer(System.get_env("CATALOG_SYNC_INTERVAL_HOURS", "168"))
config :the_gathering, :catalog_sync_interval_ms, catalog_sync_hours * 60 * 60 * 1_000

split_urls = fn name ->
  System.get_env(name, "")
  |> String.split(",", trim: true)
  |> Enum.map(&String.trim/1)
end

config :the_gathering, :webcam_table,
  stun_urls: split_urls.("WEBRTC_STUN_URLS"),
  turn_urls: split_urls.("WEBRTC_TURN_URLS"),
  turn_username: System.get_env("WEBRTC_TURN_USERNAME"),
  turn_credential: System.get_env("WEBRTC_TURN_CREDENTIAL")

# Optional origin of a self-hosted ManaVault instance whose shared deck links should be
# recognized and resolved, e.g. https://manavault.example.com. Unset disables ManaVault links.
if manavault_url = System.get_env("MANAVAULT_URL") do
  config :the_gathering, TheGathering.Decklists, manavault_url: manavault_url
end

manavault_allowed_hosts =
  System.get_env("MANAVAULT_ALLOWED_HOSTS", "")
  |> String.split(",", trim: true)
  |> Enum.map(&(String.trim(&1) |> String.downcase()))
  |> Enum.reject(&(&1 == ""))

config :the_gathering,
  manavault_allowed_hosts: manavault_allowed_hosts,
  manavault_allow_insecure_urls: System.get_env("MANAVAULT_ALLOW_INSECURE_URLS") in ["true", "1"]

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
  # LOG_LEVEL=debug shows why the Discord bot ignored a message, among other detail.
  log_level = System.get_env("LOG_LEVEL", "info")

  if log_level in ~w(debug info warning error) do
    config :logger, level: String.to_atom(log_level)
  else
    raise "LOG_LEVEL must be one of debug, info, warning, error; got #{inspect(log_level)}"
  end

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
