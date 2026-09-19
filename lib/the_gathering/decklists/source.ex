defmodule TheGathering.Decklists.Source do
  @moduledoc "Adapter contract for public deck-list metadata providers."

  alias TheGathering.Decklists.Decklist

  @type parsed_url :: %{source: atom(), id: String.t(), canonical_url: String.t()}
  @type error :: :not_found | :private | :upstream_error

  @callback resolve(parsed_url()) :: {:ok, Decklist.t()} | {:error, error()}
end
