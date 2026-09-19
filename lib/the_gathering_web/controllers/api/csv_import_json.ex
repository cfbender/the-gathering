defmodule TheGatheringWeb.API.CSVImportJSON do
  def preview(%{preview: preview}), do: %{data: preview}
  def result(%{result: result}), do: %{data: result}
end
