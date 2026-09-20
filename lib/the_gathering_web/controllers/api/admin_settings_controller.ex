defmodule TheGatheringWeb.API.AdminSettingsController do
  use TheGatheringWeb, :controller

  alias TheGathering.Accounts

  action_fallback TheGatheringWeb.API.FallbackController

  def show(conn, _params), do: render_settings(conn, Accounts.get_settings())

  def update(conn, %{"settings" => attrs}) do
    with {:ok, settings} <- Accounts.update_settings(attrs) do
      render_settings(conn, settings)
    end
  end

  def update(_conn, _params), do: {:error, :bad_request}

  defp render_settings(conn, settings) do
    json(conn, %{
      data: %{
        registration_enabled: settings.registration_enabled,
        detailed_stats_from: settings.detailed_stats_from
      }
    })
  end
end
