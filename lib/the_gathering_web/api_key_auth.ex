defmodule TheGatheringWeb.ApiKeyAuth do
  @moduledoc """
  Authenticates `Authorization: Bearer tg_…` requests as the key's owner.

  The key assigns the same `current_scope` a cookie session would, built from the
  owner's current record, so every downstream permission check treats the request
  exactly like that user. Keys never satisfy sudo mode because the scope carries no
  recent password authentication.
  """
  import Plug.Conn

  alias TheGathering.Accounts
  alias TheGathering.Accounts.{Scope, User}
  alias TheGatheringWeb.API.FallbackController

  @behaviour Plug

  @impl Plug
  def init(opts), do: opts

  @impl Plug
  def call(conn, _opts) do
    with ["Bearer " <> token] <- get_req_header(conn, "authorization"),
         %User{} = user <- Accounts.authenticate_api_key(String.trim(token)) do
      conn
      |> assign(:current_scope, Scope.for_user(user))
      |> put_resp_header("cache-control", "private, no-store")
    else
      _missing_or_invalid ->
        conn
        |> put_resp_header("www-authenticate", ~s(Bearer realm="the-gathering"))
        |> FallbackController.call({:error, :unauthorized})
        |> halt()
    end
  end
end
