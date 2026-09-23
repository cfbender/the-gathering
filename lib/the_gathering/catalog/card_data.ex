defmodule TheGathering.Catalog.CardData do
  @moduledoc false

  @pairing_rules [
    {"friends_forever", ~r/(?:^|\n)Friends forever(?:\n|$)/i},
    {"choose_a_background", ~r/(?:^|\n)Choose a Background(?:\n|$)/i},
    {"partner_with", ~r/(?:^|\n)Partner with /i},
    {"partner", ~r/(?:^|\n)Partner(?:\s|\n|$)/i}
  ]

  def from_scryfall(%{"set_type" => set_type}) when set_type in ["token", "memorabilia"],
    do: nil

  def from_scryfall(%{"id" => id, "oracle_id" => oracle_id, "name" => name} = card) do
    type_line = Map.get(card, "type_line", "")
    oracle_text = Map.get(card, "oracle_text", "")
    front = card |> Map.get("card_faces", []) |> List.first() || %{}
    image_uris = Map.get(card, "image_uris") || Map.get(front, "image_uris") || %{}
    now = DateTime.utc_now() |> DateTime.truncate(:second)

    %{
      id: id,
      oracle_id: oracle_id,
      name: name,
      normalized_name: normalize_name(name),
      mana_cost: Map.get(card, "mana_cost") || Map.get(front, "mana_cost"),
      cmc: number(Map.get(card, "cmc")),
      type_line: type_line,
      oracle_text: oracle_text,
      colors: Map.get(card, "colors", []),
      color_identity: Map.get(card, "color_identity", []),
      image_uris: Map.take(image_uris, ["small", "normal", "art_crop"]),
      set_code: Map.get(card, "set", ""),
      collector_number: Map.get(card, "collector_number", ""),
      released_at: date(Map.get(card, "released_at")),
      layout: Map.get(card, "layout", "normal"),
      rarity: Map.get(card, "rarity", "common"),
      game_changer: Map.get(card, "game_changer") == true,
      commander_legal: get_in(card, ["legalities", "commander"]) == "legal",
      can_be_commander: can_be_commander?(type_line, oracle_text),
      commander_pairing: commander_pairing(type_line, oracle_text),
      selection_key: selection_key(card),
      inserted_at: now,
      updated_at: now
    }
  end

  def from_scryfall(_card), do: nil

  def normalize_name(value) when is_binary(value) do
    value
    |> String.normalize(:nfd)
    |> String.replace(~r/\p{Mn}/u, "")
    |> String.downcase()
  end

  def can_be_commander?(type_line, oracle_text) do
    background? = Regex.match?(~r/(?:^|\s|—)Background(?:\s|$)/i, type_line)

    legendary_creature? =
      String.contains?(type_line, "Legendary") and String.contains?(type_line, "Creature")

    explicit? = Regex.match?(~r/can be your commander/i, oracle_text)

    not background? and (legendary_creature? or explicit?)
  end

  def commander_pairing(type_line, oracle_text) do
    if Regex.match?(~r/(?:^|\s|—)Background(?:\s|$)/i, type_line) do
      "background"
    else
      Enum.find_value(@pairing_rules, &pairing_value(&1, oracle_text))
    end
  end

  # The lexicographically greatest key wins: English, paper, non-digital,
  # non-promo, then newest release date, set, collector number, and UUID.
  def selection_key(card) do
    [
      bit(Map.get(card, "lang") == "en"),
      bit("paper" in Map.get(card, "games", [])),
      bit(not Map.get(card, "digital", false)),
      bit(not Map.get(card, "promo", false)),
      Map.get(card, "released_at", "0000-00-00"),
      Map.get(card, "set", ""),
      Map.get(card, "collector_number", ""),
      Map.get(card, "id", "")
    ]
    |> Enum.join("|")
  end

  defp bit(true), do: "1"
  defp bit(false), do: "0"

  defp pairing_value({value, pattern}, oracle_text),
    do: Regex.match?(pattern, oracle_text) && value

  defp number(value) when is_number(value), do: value / 1
  defp number(_value), do: 0.0

  defp date(value) when is_binary(value) do
    case Date.from_iso8601(value) do
      {:ok, date} -> date
      _error -> nil
    end
  end

  defp date(_value), do: nil
end
