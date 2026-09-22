defmodule TheGatheringWeb.UserSocket do
  use Phoenix.Socket

  alias TheGathering.Accounts

  @token_salt "webcam table socket"

  channel "webcam_table:*", TheGatheringWeb.WebcamTableChannel

  @impl true
  def connect(%{"token" => signed_token}, socket, _connect_info) do
    with {:ok, session_token} <-
           Phoenix.Token.verify(socket, @token_salt, signed_token, max_age: 86_400),
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
