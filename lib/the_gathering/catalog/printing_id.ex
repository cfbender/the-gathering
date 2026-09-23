defmodule TheGathering.Catalog.PrintingId do
  @moduledoc "Scryfall UUIDs, with `-1` for the second face (side or half) of a gallery printing."

  @id ~r/\A([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})(-1)?\z/

  def parse(id) when is_binary(id) do
    case Regex.run(@id, id) do
      [_, card_id] -> {:ok, card_id, 0}
      [_, card_id, "-1"] -> {:ok, card_id, 1}
      _ -> {:error, :bad_request}
    end
  end

  def parse(_), do: {:error, :bad_request}
end
