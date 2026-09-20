defmodule TheGathering.Catalog.BackfillTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.Catalog.{Backfill, Card, CardData}
  alias TheGathering.{Games, Repo}
  alias TheGathering.Games.LinkCatalogCards

  defp card(id, name, opts) do
    Repo.insert!(%Card{
      id: id,
      oracle_id: "oracle-#{id}",
      name: name,
      normalized_name: CardData.normalize_name(name),
      cmc: 0.0,
      type_line: Keyword.get(opts, :type_line, "Legendary Creature"),
      colors: [],
      color_identity: Keyword.get(opts, :colors, []),
      image_uris: %{},
      set_code: "tst",
      collector_number: id,
      layout: "normal",
      rarity: "rare",
      released_at: Keyword.get(opts, :released_at, ~D[2024-01-01]),
      commander_legal: true,
      can_be_commander: Keyword.get(opts, :commander, true)
    })
  end

  setup do
    card("frodo", "Frodo, Adventurous Hobbit", colors: ["W", "G"])
    card("sam", "Sam, Loyal Attendant", colors: ["W", "B"])
    card("eowyn", "Éowyn, Shieldmaiden", colors: ["W"])
    card("a-alrund", "A-Alrund, God of the Cosmos // A-Hakka, Whispering Raven", colors: ["U"])
    card("alrund", "Alrund, God of the Cosmos // Hakka, Whispering Raven", colors: ["U"])
    card("angel-token", "Angel", commander: false, type_line: "Token Creature")
    card("angel", "Angel", colors: ["W"], type_line: "Legendary Creature")
    card("rhystic", "Rhystic Study", commander: false, type_line: "Enchantment")

    {:ok, player} = Games.create_player(%{name: "Cody"})
    %{player: player}
  end

  test "splits piped partners, links both cards, and renames default deck names", %{player: p} do
    {:ok, deck} =
      Games.create_deck(%{
        player_id: p.id,
        name: "Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)",
        commander_name: "Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)",
        color_identity: "WBG"
      })

    {:ok, custom} =
      Games.create_deck(%{
        player_id: p.id,
        name: "Hobbit friends",
        commander_name: "Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)"
      })

    summary = Backfill.run()
    assert %{decks_split: 2, decks_linked: 2, colors_filled: 1, unmatched: []} = summary

    deck = Games.get_deck!(deck.id)
    assert deck.name == "Frodo, Adventurous Hobbit / Sam, Loyal Attendant"
    assert deck.commander_name == "Frodo, Adventurous Hobbit"
    assert deck.partner_name == "Sam, Loyal Attendant"
    assert deck.commander_card_id == "frodo"
    assert deck.partner_card_id == "sam"
    # An existing color identity is trusted, not recomputed.
    assert deck.color_identity == "WBG"

    custom = Games.get_deck!(custom.id)
    assert custom.name == "Hobbit friends"
    assert custom.color_identity == "WBG"
  end

  test "matches accented names, front faces, and prefers real commanders over tokens", %{
    player: p
  } do
    {:ok, eowyn} =
      Games.create_deck(%{player_id: p.id, name: "E", commander_name: "Eowyn, Shieldmaiden"})

    {:ok, alrund} =
      Games.create_deck(%{
        player_id: p.id,
        name: "A",
        commander_name: "Alrund, God of the Cosmos"
      })

    {:ok, angel} = Games.create_deck(%{player_id: p.id, name: "Ang", commander_name: "Angel"})

    {:ok, missing} =
      Games.create_deck(%{player_id: p.id, name: "M", commander_name: "Nobody Here"})

    summary = Backfill.run()
    assert summary.decks_linked == 3
    assert summary.unmatched == ["Nobody Here"]

    assert Games.get_deck!(eowyn.id).commander_card_id == "eowyn"
    assert Games.get_deck!(alrund.id).commander_card_id == "alrund"
    assert Games.get_deck!(angel.id).commander_card_id == "angel"
    assert Games.get_deck!(missing.id).commander_card_id == nil

    # Rerunning is a no-op apart from retrying the unmatched deck.
    assert %{decks_linked: 0, decks_split: 0, unmatched: ["Nobody Here"]} = Backfill.run()
  end

  test "links MVP cards recorded by name only", %{player: p} do
    {:ok, other} = Games.create_player(%{name: "Jules"})

    {:ok, deck} =
      Games.create_deck(%{
        player_id: p.id,
        name: "F",
        commander_name: "Frodo, Adventurous Hobbit"
      })

    {:ok, deck2} =
      Games.create_deck(%{player_id: other.id, name: "S", commander_name: "Sam, Loyal Attendant"})

    {:ok, game} =
      Games.create_game(%{
        played_at: ~U[2026-03-01 18:00:00Z],
        source: "csv",
        seats: [
          %{
            player_id: p.id,
            deck_id: deck.id,
            seat: 1,
            result: "win",
            mvp_card_name: "Rhystic Study"
          },
          %{player_id: other.id, deck_id: deck2.id, seat: 2, result: "loss"}
        ]
      })

    assert %{mvps_linked: 1} = Backfill.run()

    winner =
      game.id |> Games.get_game!() |> Map.fetch!(:seats) |> Enum.find(&(&1.result == "win"))

    assert winner.mvp_card_id == "rhystic"
  end

  test "bounded repair returns a cursor and reports update conflicts", %{player: p} do
    {:ok, _existing} =
      Games.create_deck(%{
        player_id: p.id,
        name: "Frodo, Adventurous Hobbit / Sam, Loyal Attendant",
        commander_name: "Frodo, Adventurous Hobbit",
        commander_card_id: "frodo"
      })

    {:ok, piped} =
      Games.create_deck(%{
        player_id: p.id,
        name: "Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)",
        commander_name: "Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)"
      })

    assert {:ok, first} =
             LinkCatalogCards.repair_batch(%{deck_id: 0, seat_id: 0}, limit: 1)

    refute first.done?
    assert [%{resource: :deck, id: id, errors: errors}] = first.conflicts
    assert id == piped.id
    assert Keyword.has_key?(errors, :name)
    assert Games.get_deck!(piped.id).commander_card_id == nil

    assert {:ok, second} = LinkCatalogCards.repair_batch(first.cursor, limit: 1)
    assert second.done?
  end
end
