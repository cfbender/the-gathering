defmodule TheGatheringWeb.API.AdminSoftwareUpdateJSON do
  def show(%{status: status}) do
    %{
      data: %{
        version: status.version,
        channel: status.channel,
        method: status.method,
        pending: status.pending,
        requested_at: status.requested_at,
        latest: status.latest,
        update_available: status.update_available,
        check_error: status.check_error
      }
    }
  end
end
