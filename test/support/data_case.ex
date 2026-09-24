defmodule TheGathering.DataCase do
  @moduledoc """
  This module defines the setup for tests requiring
  access to the application's data layer.

  You may define functions here to be used as helpers in
  your tests.

  Finally, if the test case interacts with the database,
  we enable the SQL sandbox, so changes done to the database
  are reverted at the end of every test. If you are using
  PostgreSQL, you can even run database tests asynchronously
  by setting `use TheGathering.DataCase, async: true`, although
  this option is not recommended for other databases.
  """

  use ExUnit.CaseTemplate

  alias Ecto.Adapters.SQL.Sandbox

  using do
    quote do
      alias TheGathering.Repo

      import Ecto
      import Ecto.Changeset
      import Ecto.Query
      import TheGathering.DataCase
    end
  end

  setup tags do
    TheGathering.DataCase.setup_sandbox(tags)
    :ok
  end

  @doc """
  Sets up the sandbox based on the test tags.
  """
  def setup_sandbox(tags) do
    pid = Sandbox.start_owner!(TheGathering.Repo, shared: not tags[:async])
    on_exit(fn -> Sandbox.stop_owner(pid) end)

    # Webcam table rooms outlive their connections and write through the shared
    # sandbox, so stop them before it closes (on_exit runs in reverse order).
    unless tags[:async], do: on_exit(&stop_webcam_table_rooms/0)
  end

  defp stop_webcam_table_rooms do
    supervisor = TheGathering.WebcamTables.RoomSupervisor

    for {_id, pid, _type, _modules} <- DynamicSupervisor.which_children(supervisor),
        do: DynamicSupervisor.terminate_child(supervisor, pid)
  end

  @doc """
  A helper that transforms changeset errors into a map of messages.

      assert {:error, changeset} = Accounts.create_user(%{password: "short"})
      assert "password is too short" in errors_on(changeset).password
      assert %{password: ["password is too short"]} = errors_on(changeset)

  """
  def errors_on(changeset) do
    Ecto.Changeset.traverse_errors(changeset, fn {message, opts} ->
      Regex.replace(~r"%{(\w+)}", message, fn _, key ->
        opts |> Keyword.get(String.to_existing_atom(key), key) |> to_string()
      end)
    end)
  end
end
