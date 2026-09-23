defmodule TheGatheringWeb.API.CardIdCorrectionJSON do
  def show(%{correction: correction}), do: %{data: correction}
  def index(%{page: page}), do: %{data: page}
end
