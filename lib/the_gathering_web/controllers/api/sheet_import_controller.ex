defmodule TheGatheringWeb.API.SheetImportController do
  use TheGatheringWeb, :controller
  alias TheGathering.Imports

  action_fallback TheGatheringWeb.API.FallbackController

  def preview(conn, params) do
    with :ok <- validate(params),
         {:ok, preview} <- Imports.preview_sheet(params) do
      render(conn, :show, data: preview)
    else
      error -> import_error(error)
    end
  end

  def create(conn, params) do
    {revision, input} = Map.pop(params, "revision")

    with :ok <- validate(input),
         {:ok, result} <-
           Imports.import_sheet(input, revision, conn.assigns.current_scope.user.id) do
      render(conn, :show, data: result)
    else
      error -> import_error(error)
    end
  end

  defp validate(%{"text" => text} = params) when is_binary(text) do
    choices_valid =
      Enum.all?(~w(players decks actions), fn key ->
        choices = Map.get(params, key, %{})

        is_map(choices) and
          Enum.all?(choices, fn {_key, value} ->
            is_integer(value) or value in ["new", "skip", "create"]
          end)
      end)

    if choices_valid, do: :ok, else: {:error, :bad_request}
  end

  defp validate(_), do: {:error, :bad_request}

  defp import_error({:error, message}) when is_binary(message) do
    {:error,
     Ecto.Changeset.add_error(Ecto.Changeset.change({%{}, %{import: :string}}), :import, message)}
  end

  defp import_error(error), do: error
end
