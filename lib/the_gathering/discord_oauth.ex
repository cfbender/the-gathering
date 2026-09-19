defmodule TheGathering.DiscordOAuth do
  @moduledoc "Configuration boundary for Discord OAuth sign-in."

  require Logger

  alias TheGatheringWeb.Endpoint

  def configured? do
    config = Application.get_env(:the_gathering, :discord_oauth, [])
    present?(config[:client_id]) and present?(config[:client_secret])
  end

  @doc """
  Logs once at boot whether member sign-in is available, so `docker compose logs`
  explains a login page that shows "Discord sign-in is not configured".
  """
  def log_status do
    if configured?() do
      Logger.info(
        "Discord OAuth sign-in enabled; redirect URI is #{Endpoint.url()}/auth/discord/callback"
      )
    else
      Logger.info(
        "Discord OAuth sign-in disabled: DISCORD_CLIENT_ID and DISCORD_CLIENT_SECRET are not both set"
      )
    end

    :ok
  end

  def config do
    Application.get_env(:the_gathering, :discord_oauth, [])
    |> Keyword.put_new(:redirect_uri, Endpoint.url() <> "/auth/discord/callback")
    |> Keyword.put_new(:http_adapter, Assent.HTTPAdapter.Req)
  end

  defp present?(value), do: is_binary(value) and value != ""
end
