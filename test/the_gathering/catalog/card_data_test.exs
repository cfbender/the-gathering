defmodule TheGathering.Catalog.CardDataTest do
  use ExUnit.Case, async: true

  alias TheGathering.Catalog.CardData

  test "copies the Scryfall Game Changer flag, defaulting missing values to false" do
    card = %{"id" => "rhystic", "oracle_id" => "oracle-rhystic", "name" => "Rhystic Study"}
    assert CardData.from_scryfall(Map.put(card, "game_changer", true)).game_changer
    refute CardData.from_scryfall(Map.put(card, "game_changer", false)).game_changer
    refute CardData.from_scryfall(card).game_changer
  end

  test "derives commander eligibility without treating Backgrounds as commanders" do
    assert CardData.can_be_commander?("Legendary Creature — Human Wizard", "")
    refute CardData.can_be_commander?("Legendary Artifact", "")

    assert CardData.can_be_commander?(
             "Legendary Planeswalker — Test",
             "Test can be your commander."
           )

    refute CardData.can_be_commander?("Legendary Enchantment — Background", "")
  end

  test "represents partner mechanics separately from commander eligibility" do
    assert CardData.commander_pairing("Legendary Creature — Human", "Partner") == "partner"

    assert CardData.commander_pairing("Legendary Creature — Human", "Friends forever") ==
             "friends_forever"

    assert CardData.commander_pairing("Legendary Creature — Human", "Choose a Background") ==
             "choose_a_background"

    assert CardData.commander_pairing("Legendary Enchantment — Background", "") == "background"
  end
end
