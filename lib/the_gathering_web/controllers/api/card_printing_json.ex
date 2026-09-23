defmodule TheGatheringWeb.API.CardPrintingJSON do
  alias TheGathering.Catalog.CardImages

  def index(%{printings: printings, has_more: has_more}),
    do: %{data: Enum.map(printings, &summary/1), has_more: has_more}

  def show(%{printing: printing}), do: %{data: summary(printing)}

  def rulings(%{rulings: rulings}), do: %{data: rulings}

  def details(%{details: details}) do
    %{
      data:
        Map.take(details, [
          :id,
          :oracle_id,
          :name,
          :set_code,
          :set_name,
          :collector_number,
          :lang,
          :image_uris,
          :mana_cost,
          :type_line,
          :oracle_text,
          :flavor_text,
          :power,
          :toughness,
          :loyalty,
          :layout,
          :rarity,
          :released_at,
          :prices,
          :scryfall_uri
        ])
        |> Map.update!(:image_uris, &CardImages.urls/1)
    }
  end

  defp summary(printing) do
    Map.take(printing, [:id, :name, :set_code, :set_name, :collector_number, :lang, :image_uris])
    |> Map.update!(:image_uris, &CardImages.urls/1)
  end
end
