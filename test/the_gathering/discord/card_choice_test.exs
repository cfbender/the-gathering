defmodule TheGathering.Discord.CardChoiceTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.Catalog.{Card, CardData}
  alias TheGathering.Discord.CardChoice

  test "resolves a unique whole leading name among more than 25 broad matches" do
    insert_card("bello", "Bello, Bard of the Brambles")

    for number <- 1..25 do
      insert_card("decoy-#{number}", "Bellowing Decoy #{number}")
    end

    assert %{"id" => "bello", "name" => "Bello, Bard of the Brambles", "error" => nil} =
             CardChoice.resolve("Bello", "commander")
  end

  test "keeps multiple whole leading names ambiguous while resolving front-face shorthand" do
    insert_card("sephiroth-fabled", "Sephiroth, Fabled SOLDIER // Sephiroth, One-Winged Angel")
    insert_card("sephiroth-heir", "Sephiroth, Planet's Heir")
    insert_card("terra", "Terra, Magical Adept // Esper Terra")

    assert %{"id" => nil, "candidates" => [_, _], "error" => error} =
             CardChoice.resolve("Sephiroth", "commander")

    assert error == "Choose a matching commander card below."

    assert %{"id" => "sephiroth-fabled"} =
             CardChoice.resolve("sephiroth fabled soldier", "commander")

    assert %{"id" => "terra"} = CardChoice.resolve("Terra magical adept", "commander")
  end

  defp insert_card(id, name) do
    attrs =
      %{
        "id" => id,
        "oracle_id" => "oracle-#{id}",
        "name" => name,
        "lang" => "en",
        "games" => ["paper"],
        "released_at" => "2024-01-01",
        "set" => "tst",
        "collector_number" => id,
        "type_line" => "Legendary Creature — Human",
        "layout" => "normal",
        "rarity" => "rare",
        "legalities" => %{"commander" => "legal"}
      }
      |> CardData.from_scryfall()
      |> Map.delete(:selection_key)

    Repo.insert_all(Card, [attrs])
  end
end
