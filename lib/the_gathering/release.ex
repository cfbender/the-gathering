defmodule TheGathering.Release do
  @moduledoc false

  alias TheGathering.Accounts

  def bootstrap_admin do
    {:ok, _started} = Application.ensure_all_started(:the_gathering)

    username = System.fetch_env!("THE_GATHERING_ADMIN_USERNAME")
    password = System.fetch_env!("THE_GATHERING_ADMIN_PASSWORD")

    case Accounts.get_user_by_username(username) do
      %{role: "admin"} ->
        :ok

      nil ->
        case Accounts.create_admin(%{
               "username" => username,
               "display_name" => username,
               "password" => password
             }) do
          {:ok, _user} ->
            :ok

          {:error, changeset} ->
            raise "could not create bootstrap admin: #{inspect(changeset.errors)}"
        end

      _user ->
        raise "bootstrap username already belongs to a non-admin account"
    end
  end
end
