defmodule TheGatheringWeb.DiscordAuthController do
  use TheGatheringWeb, :controller

  require Logger

  alias Assent.Strategy.Discord
  alias TheGathering.{Accounts, DiscordOAuth}
  alias TheGatheringWeb.UserAuth

  @oauth_session :discord_oauth

  def request(conn, params) do
    with true <- DiscordOAuth.configured?(),
         {:ok, mode} <- oauth_mode(conn, params),
         {:ok, %{url: url, session_params: session_params}} <-
           Discord.authorize_url(DiscordOAuth.config()) do
      oauth_session = %{
        session_params: session_params,
        return_to: safe_return_to(params["returnTo"]),
        sudo_discord_id: mode,
        registration_invite_hash: get_session(conn, :registration_invite_hash)
      }

      # Keep the invitation available if OAuth is canceled or restarted. Each
      # attempt still binds its own digest and rechecks it during registration.
      conn
      |> put_session(@oauth_session, oauth_session)
      |> redirect(external: url)
    else
      false -> login_error(conn, "discord_unavailable")
      {:error, :discord_sudo_unavailable} -> login_error(conn, "discord_sudo_unavailable")
      {:error, _reason} -> login_error(conn, "discord_failed")
    end
  end

  def callback(conn, params) do
    oauth_session = get_session(conn, @oauth_session)
    conn = delete_session(conn, @oauth_session)

    with %{session_params: session_params} <- oauth_session,
         {:ok, %{user: claims}} <-
           Discord.callback(
             Keyword.put(DiscordOAuth.config(), :session_params, session_params),
             params
           ),
         :ok <- verify_sudo_identity(oauth_session.sudo_discord_id, claims),
         {:ok, user} <-
           Accounts.sign_in_with_discord(
             claims,
             Map.get(oauth_session, :registration_invite_hash)
           ) do
      conn
      |> delete_session(:registration_invite_hash)
      |> UserAuth.log_in_user(user)
      |> redirect(to: oauth_session.return_to)
    else
      {:error, :registration_closed} ->
        Logger.info("Discord sign-in rejected an unknown account because registration is closed")
        login_error(conn, "registration_closed")

      {:error, :disabled} ->
        login_error(conn, "account_disabled")

      {:error, :wrong_sudo_user} ->
        login_error(conn, "discord_sudo_mismatch")

      error ->
        Logger.warning("Discord sign-in failed: #{oauth_error_summary(error)}")
        login_error(conn, "discord_failed")
    end
  end

  defp oauth_error_summary({:error, %{__struct__: module, response: %{status: status}}})
       when is_integer(status),
       do: "#{inspect(module)} status=#{status}"

  defp oauth_error_summary({:error, %{__struct__: module}}), do: inspect(module)

  defp oauth_error_summary({:error, {:invalid_user, %Ecto.Changeset{errors: errors}}}),
    do: "invalid user #{inspect(Keyword.keys(errors))}"

  defp oauth_error_summary(_error), do: "unknown OAuth error"

  defp oauth_mode(conn, %{"sudo" => "1"}) do
    case conn.assigns.current_scope.user do
      %{discord_id: discord_id} when not is_nil(discord_id) -> {:ok, discord_id}
      _user -> {:error, :discord_sudo_unavailable}
    end
  end

  defp oauth_mode(_conn, _params), do: {:ok, nil}

  defp verify_sudo_identity(nil, _claims), do: :ok
  defp verify_sudo_identity(discord_id, %{"sub" => discord_id}), do: :ok
  defp verify_sudo_identity(_discord_id, _claims), do: {:error, :wrong_sudo_user}

  defp safe_return_to(path) when is_binary(path) do
    if String.starts_with?(path, "/") and not String.starts_with?(path, "//"), do: path, else: "/"
  end

  defp safe_return_to(_path), do: "/"

  defp login_error(conn, error) do
    redirect(conn, to: "/login?" <> URI.encode_query(%{"error" => error}))
  end
end
