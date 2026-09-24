defmodule TheGatheringWeb.UserSocket do
  use Phoenix.Socket

  alias TheGathering.Accounts

  @token_salt "webcam table socket"
  @token_max_age 86_400

  channel "webcam_table:*", TheGatheringWeb.WebcamTableChannel

  @doc """
  Encrypts the cookie session token for the browser to pass as the `token`
  connect param. Encrypted, not just signed, so page scripts cannot read the
  session token out of it.
  """
  def token(conn_or_endpoint, session_token),
    do: Phoenix.Token.encrypt(conn_or_endpoint, @token_salt, session_token)

  # The session token is looked up rather than trusted, so logging out (which
  # deletes it and broadcasts to `id/1`) also disconnects the socket.
  @impl true
  def connect(%{"token" => token}, socket, _connect_info) when is_binary(token) do
    with {:ok, session_token} <-
           Phoenix.Token.decrypt(socket, @token_salt, token, max_age: @token_max_age),
         {user, _inserted_at} <- Accounts.get_user_by_session_token(session_token) do
      {:ok, assign(socket, user: user, session_token: session_token)}
    else
      _invalid_session -> :error
    end
  end

  def connect(_params, _socket, _connect_info), do: :error

  @impl true
  def id(%{assigns: %{session_token: token}}),
    do: "users_sessions:#{Base.url_encode64(token)}"
end
