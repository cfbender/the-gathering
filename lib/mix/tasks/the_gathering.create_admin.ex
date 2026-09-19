defmodule Mix.Tasks.TheGathering.CreateAdmin do
  use Mix.Task

  alias TheGathering.Accounts

  @shortdoc "Creates an administrator account"

  @moduledoc """
  Creates an administrator account for headless server bootstrap.

      THE_GATHERING_ADMIN_PASSWORD='a long password' mix the_gathering.create_admin USERNAME

  The password is read only from the environment so it does not appear in the process list.
  """

  @impl Mix.Task
  def run([username]) do
    Mix.Task.run("app.start")
    password = System.get_env("THE_GATHERING_ADMIN_PASSWORD")

    if is_nil(password) do
      Mix.raise("THE_GATHERING_ADMIN_PASSWORD must be set")
    end

    case Accounts.create_admin(%{
           "username" => username,
           "display_name" => username,
           "password" => password
         }) do
      {:ok, user} -> Mix.shell().info("Created admin #{user.username}")
      {:error, changeset} -> Mix.raise("could not create admin: #{inspect(changeset.errors)}")
    end
  end

  def run(_args), do: Mix.raise("usage: mix the_gathering.create_admin USERNAME")
end
