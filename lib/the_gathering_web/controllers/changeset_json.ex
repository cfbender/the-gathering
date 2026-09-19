defmodule TheGatheringWeb.ChangesetJSON do
  @moduledoc ~S(Renders `Ecto.Changeset` errors as `{"errors": {"field": ["message", ...]}}`.)

  @doc """
  Renders changeset errors, interpolating `%{count}`-style placeholders.

  Nested changesets (embeds and associations) render as nested maps.
  """
  def error(%{changeset: changeset}) do
    %{errors: Ecto.Changeset.traverse_errors(changeset, &translate_error/1)}
  end

  defp translate_error({msg, opts}) do
    Regex.replace(~r"%{(\w+)}", msg, fn _match, key ->
      opts |> Keyword.get(String.to_existing_atom(key), key) |> to_string()
    end)
  end
end
