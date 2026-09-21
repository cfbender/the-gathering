defmodule TheGatheringWeb.API.CardPrintingJSON do
  def index(%{printings: printings, has_more: has_more}),
    do: %{data: Enum.map(printings, &summary/1), has_more: has_more}

  def show(%{printing: printing}), do: %{data: summary(printing)}

  defp summary(printing) do
    Map.take(printing, [:id, :name, :set_code, :set_name, :collector_number, :lang, :image_uris])
  end
end
