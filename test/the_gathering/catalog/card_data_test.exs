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

  test "recognizes current Oracle pairing wording with reminder text" do
    creature = "Legendary Creature — Human"

    assert CardData.commander_pairing(
             creature,
             "Reach\nChoose a Background (You can have a Background as a second commander.)"
           ) == "choose_a_background"

    assert CardData.commander_pairing(
             creature,
             "Goad that creature.\nPartner—Friends forever (You can have two commanders if both have this ability.)"
           ) == "friends_forever"

    assert CardData.commander_pairing(
             creature,
             "Menace\nPartner—Survivors (You can have two commanders if both have this ability.)"
           ) == "partner"

    assert CardData.commander_pairing(
             creature,
             "Draw a card.\nPartner (You can have two commanders if both have partner.)"
           ) == "partner"

    assert CardData.commander_pairing(
             creature,
             "Partner with Toothy, Imaginary Friend (When this creature enters, …)"
           ) == "partner_with"

    assert CardData.commander_pairing(
             "Legendary Creature — Human Advisor",
             "Doctor's companion (You can have two commanders if the other is the Doctor.)"
           ) == "doctors_companion"
  end

  test "marks Time Lord Doctors as pairable with a Doctor's companion" do
    assert CardData.commander_pairing("Legendary Creature — Time Lord Doctor", "Haste") ==
             "doctor"

    assert CardData.commander_pairing("Legendary Creature — Time Lord Scientist", "") == nil
    assert CardData.commander_pairing("Creature — Time Lord Doctor", "") == nil
    assert CardData.commander_pairing("Legendary Creature — Goblin Warrior", "Flying") == nil
  end

  test "ignores pairing words that are not keyword lines" do
    assert CardData.commander_pairing(
             "Legendary Creature — Human",
             "Whenever you cast a Doctor spell or creature spell with doctor's companion, draw a card."
           ) == nil

    assert CardData.commander_pairing("Legendary Creature — Human", "Partners in crime") == nil
  end
end
