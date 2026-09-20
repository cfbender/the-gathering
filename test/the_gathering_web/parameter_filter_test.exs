defmodule TheGatheringWeb.ParameterFilterTest do
  use ExUnit.Case, async: true

  import ExUnit.CaptureLog

  test "request logging redacts ManaVault and OAuth parameters" do
    secrets = %{
      "manavault_api_key" => "sentinel-manavault-key",
      "code" => "sentinel-oauth-code",
      "state" => "sentinel-oauth-state"
    }

    log =
      capture_log(fn ->
        require Logger
        Logger.warning("params=#{inspect(Phoenix.Logger.filter_values(secrets))}")
      end)

    refute log =~ "sentinel-manavault-key"
    refute log =~ "sentinel-oauth-code"
    refute log =~ "sentinel-oauth-state"
    assert log =~ "[FILTERED]"
  end
end
