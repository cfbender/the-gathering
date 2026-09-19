defmodule TheGathering.DiscordOAuth do
  @moduledoc "Configuration boundary for Discord OAuth sign-in."

  alias TheGatheringWeb.Endpoint

  def configured? do
    config = Application.get_env(:the_gathering, :discord_oauth, [])
    present?(config[:client_id]) and present?(config[:client_secret])
  end

  def config do
    Application.get_env(:the_gathering, :discord_oauth, [])
    |> Keyword.put_new(:redirect_uri, Endpoint.url() <> "/auth/discord/callback")
    |> Keyword.put_new(:http_adapter, Assent.HTTPAdapter.Req)
  end

  defp present?(value), do: is_binary(value) and value != ""
end
