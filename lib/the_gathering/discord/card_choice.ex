defmodule TheGathering.Discord.CardChoice do
  @moduledoc false

  alias TheGathering.Catalog

  def resolve(name, label, mode \\ :all) do
    name = String.trim(name || "")
    matches = matches(name, mode)
    exact = Catalog.find_card_by_name(name)
    matches = if exact && Enum.any?(matches, &(&1.id == exact.id)), do: [exact], else: matches

    choice = %{"name" => name, "id" => nil, "candidates" => [], "error" => nil}

    case {name, matches} do
      {"", _} ->
        choice

      {_, [card]} ->
        Map.merge(choice, %{"name" => card.name, "id" => card.id})

      {_, []} ->
        Map.put(choice, "error", "#{label} card not found. Edit the name or leave it blank.")

      {_, cards} when length(cards) > 25 ->
        Map.put(choice, "error", "Too many #{label} matches. Enter a more specific name.")

      {_, cards} ->
        Map.merge(choice, %{
          "candidates" => Enum.map(cards, &%{"id" => &1.id, "name" => &1.name}),
          "error" => "Choose a matching #{label} card below."
        })
    end
  end

  def choose(choice, id) do
    case Enum.find(choice["candidates"] || [], &(&1["id"] == id)) do
      nil -> {:error, "Select one of the matching cards."}
      card -> {:ok, Map.merge(choice, Map.merge(card, %{"candidates" => [], "error" => nil}))}
    end
  end

  def card(%{"name" => ""}, _label), do: {:ok, nil}

  def card(choice, label) do
    case choice["id"] && Catalog.get_card(choice["id"]) do
      nil -> {:error, choice["error"] || "Select a #{label} card or leave it blank."}
      card -> {:ok, card}
    end
  end

  defp matches("", _mode), do: []
  defp matches(name, :all), do: Catalog.search(name, limit: 26)
  defp matches(name, :commander), do: Catalog.search(name, commander: true, limit: 26)

  # Support second commanders and Backgrounds, including rule-zero pairings.
  defp matches(name, :partner) do
    (Catalog.search(name, commander: true, limit: 26) ++
       Catalog.search(name, partner: true, limit: 26))
    |> Enum.uniq_by(& &1.id)
  end
end
