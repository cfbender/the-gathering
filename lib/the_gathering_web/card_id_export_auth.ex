defmodule TheGatheringWeb.CardIdExportAuth do
  @moduledoc "Read-only correction export capability, bound to an active administrator."
  import Plug.Conn

  alias TheGathering.Accounts
  alias TheGatheringWeb.API.FallbackController

  def init(opts), do: opts

  def call(conn, _opts) do
    if authorized?(conn) do
      put_resp_header(conn, "cache-control", "private, no-store")
    else
      conn |> FallbackController.call({:error, :forbidden}) |> halt()
    end
  end

  defp authorized?(conn) do
    case get_req_header(conn, "authorization") do
      [] -> match?(%{user: %{role: "admin"}}, conn.assigns[:current_scope])
      ["Bearer " <> token] -> valid_token?(token)
      _ -> false
    end
  end

  defp valid_token?(token) do
    config = Application.get_env(:the_gathering, :cardid_corrections_export, [])
    expected = config[:token]
    admin_id = config[:admin_id]

    is_binary(expected) and byte_size(expected) >= 32 and
      Plug.Crypto.secure_compare(token, expected) and is_integer(admin_id) and
      match?(%{role: "admin", disabled_at: nil}, Accounts.get_user(admin_id))
  end
end
