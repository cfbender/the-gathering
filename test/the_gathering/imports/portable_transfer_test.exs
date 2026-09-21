defmodule TheGathering.Imports.PortableTransferTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.{AccountsFixtures, Games, Imports}
  alias TheGathering.Catalog.{Card, CardData, Printing}
  alias TheGathering.Games.{Deck, Game, GamePlayer, Player}
  alias TheGathering.Imports.SheetReceipt

  setup do
    user = AccountsFixtures.user_fixture()
    {:ok, alice} = Games.create_player(%{name: "Alice", discord_id: "private-discord"}, user.id)
    {:ok, bob} = Games.create_player(%{name: "Bob"})
    {:ok, carol} = Games.create_player(%{name: "Carol"})

    {:ok, archived} =
      Games.create_player(%{name: "Retired", archived_at: ~U[2025-01-01 00:00:00Z]})

    card =
      %{
        "id" => "card-identity",
        "oracle_id" => "oracle-commander",
        "name" => "Karn, Silver Golem",
        "lang" => "en",
        "games" => ["paper"],
        "released_at" => "2024-01-01",
        "set" => "tst",
        "collector_number" => "1",
        "type_line" => "Legendary Artifact Creature — Golem",
        "layout" => "normal",
        "rarity" => "rare",
        "legalities" => %{"commander" => "legal"}
      }
      |> CardData.from_scryfall()
      |> Map.delete(:selection_key)

    Repo.insert_all(Card, [card])

    Repo.insert!(%Printing{
      id: "chosen-art",
      oracle_id: "oracle-commander",
      name: "Karn, Silver Golem",
      set_code: "tst",
      set_name: "Test",
      collector_number: "2",
      lang: "en",
      image_uris: %{"art_crop" => "https://example.com/karn.jpg"}
    })

    {:ok, deck} =
      Games.create_deck(%{
        player_id: alice.id,
        name: "Metal friends",
        commander_name: "Karn, Silver Golem",
        commander_card_id: "card-identity",
        commander_printing_id: "chosen-art",
        decklist_url: "https://moxfield.com/decks/example",
        included_for_play: false,
        archived_at: ~U[2025-02-01 00:00:00Z]
      })

    deck |> change(skip_count: 4) |> Repo.update!()

    {:ok, _} =
      Games.create_deck(%{
        player_id: archived.id,
        name: "Unused",
        commander_name: "Karn, Silver Golem"
      })

    attrs = %{
      played_at: ~U[2025-03-17 21:04:00Z],
      turns: 11,
      duration_minutes: 83,
      notes: "Line one\nLine two",
      seats: [
        %{
          player_id: bob.id,
          seat: 1,
          result: "loss",
          kills: nil,
          eliminated_turn: 8,
          eliminated_by_player_id: alice.id,
          notes: "Seat note"
        },
        %{
          player_id: alice.id,
          deck_id: deck.id,
          seat: 2,
          result: "win",
          kills: 2,
          mvp_card_name: "Karn, Silver Golem",
          mvp_card_id: "card-identity"
        },
        %{player_id: carol.id, seat: 3, result: "loss", kills: 0}
      ]
    }

    {:ok, first} = Games.create_game(attrs, user.id)
    {:ok, second} = Games.create_game(attrs, user.id)

    {:ok, imported} =
      Games.create_game(Map.merge(attrs, %{source: "mythic_track", external_id: "original-game"}))

    Repo.insert!(%SheetReceipt{key: "reviewed-sheet-row", game_id: first.id})
    %{first: first, second: second, imported: imported, alice: alice, deck: deck}
  end

  test "round trip preserves gameplay, unused records, art and receipts with different local IDs",
       ctx do
    {:ok, export} = Imports.export_portable()
    json = Jason.encode!(export)
    refute json =~ "private-discord"
    refute json =~ "user_id"
    refute json =~ "created_by"
    assert length(export.games) == 3
    assert ctx.first.portable_id != ctx.second.portable_id
    clear_history()
    {:ok, _unrelated} = Games.create_player(%{name: "Unrelated"})

    assert {:ok, %{players: %{created: 4}, decks: %{created: 2}, games: %{created: 3}}} =
             Imports.preview_portable(json)

    assert Repo.aggregate(Player, :count) == 1
    assert Repo.aggregate(Game, :count) == 0
    assert Repo.aggregate(Card, :count) == 0
    assert {:ok, %{games: %{created: 3}}} = Imports.import_portable(json, nil)

    alice = Repo.get_by!(Player, name: "Alice")
    refute alice.id == ctx.alice.id
    assert alice.user_id == nil
    assert alice.discord_id == nil

    saved =
      Repo.get_by!(Game, portable_id: ctx.first.portable_id)
      |> Repo.preload(seats: [:player, :deck])

    assert saved.played_at == ~U[2025-03-17 21:04:00Z]

    assert {saved.duration_minutes, saved.turns, saved.notes} ==
             {83, 11, "Line one\nLine two"}

    seats = Enum.sort_by(saved.seats, & &1.seat)

    assert Enum.map(seats, &{&1.player.name, &1.result, &1.kills}) == [
             {"Bob", "loss", nil},
             {"Alice", "win", 2},
             {"Carol", "loss", 0}
           ]

    assert hd(seats).eliminated_by_player_id == alice.id
    assert hd(seats).eliminated_turn == 8
    assert hd(seats).notes == "Seat note"
    assert Enum.at(seats, 1).mvp_card_id == "card-identity"
    deck = Enum.at(seats, 1).deck

    assert {deck.name, deck.commander_printing_id, deck.skip_count, deck.included_for_play} ==
             {"Metal friends", "chosen-art", 4, false}

    assert deck.archived_at == ~U[2025-02-01 00:00:00Z]
    assert deck.decklist_source == "moxfield"
    assert Repo.get_by!(Player, name: "Retired").archived_at == ~U[2025-01-01 00:00:00Z]
    assert Repo.get!(SheetReceipt, "reviewed-sheet-row").game_id == saved.id

    assert Repo.get!(Printing, "chosen-art").image_uris["art_crop"] ==
             "https://example.com/karn.jpg"

    assert Repo.get_by!(Game, portable_id: ctx.imported.portable_id).external_id ==
             "original-game"

    {:ok, _} = Games.update_game(saved, %{notes: "Edited on destination"})
    assert {:ok, %{games: %{created: 0, reused: 3}}} = Imports.import_portable(json, nil)
    {:ok, reexport} = Imports.export_portable()

    assert {:ok, %{games: %{created: 0, reused: 3}}} =
             Imports.import_portable(Jason.encode!(reexport), nil)

    assert Repo.get!(Game, saved.id).notes == "Edited on destination"
    assert Repo.aggregate(Game, :count) == 3
  end

  test "malformed and unsupported files reject without writes" do
    {:ok, export} = Imports.export_portable()
    data = export |> Jason.encode!() |> Jason.decode!()
    clear_history()

    for invalid <- [
          "bad",
          "[]",
          Jason.encode!(%{data | "version" => 99}),
          Jason.encode!(%{data | "games" => [%{"seats" => nil}]}),
          Jason.encode!(%{data | "players" => data["players"] ++ data["players"]})
        ] do
      assert {:error, _} = Imports.preview_portable(invalid)
      assert {:error, _} = Imports.import_portable(invalid, nil)
    end

    assert Repo.aggregate(Player, :count) == 0
  end

  test "source identities recognize independent imports and conflicting identities block", ctx do
    {:ok, export} = Imports.export_portable()
    data = export |> Jason.encode!() |> Jason.decode!()
    independent = put_in(data, ["games", Access.at(2), "portable_id"], Ecto.UUID.generate())

    assert {:ok, %{games: %{created: 0, reused: 3}}} =
             Imports.import_portable(Jason.encode!(independent), nil)

    assert Repo.get!(Game, ctx.imported.id).portable_id == ctx.imported.portable_id

    conflict =
      update_in(data, ["games", Access.at(0)], fn game ->
        Map.merge(game, %{"source" => "mythic_track", "external_id" => "original-game"})
      end)

    assert {:error, "Game identities refer to different existing games."} =
             Imports.import_portable(Jason.encode!(conflict), nil)

    assert Repo.aggregate(Game, :count) == 3
  end

  test "null deck selection settings fail validation rather than database constraints" do
    {:ok, export} = Imports.export_portable()
    data = export |> Jason.encode!() |> Jason.decode!()
    clear_history()

    for field <- ["skip_count", "included_for_play"] do
      invalid = put_in(data, ["decks", Access.at(0), field], nil)
      assert {:error, _} = Imports.import_portable(Jason.encode!(invalid), nil)
      assert Repo.aggregate(Player, :count) == 0
    end
  end

  test "bad references, invalid kills and deck ownership roll back the entire file" do
    {:ok, export} = Imports.export_portable()
    data = export |> Jason.encode!() |> Jason.decode!()
    clear_history()

    for changes <- [
          %{"player_id" => 999_999},
          %{"kills" => -1},
          %{"deck_id" => hd(data["decks"])["id"]}
        ] do
      bad =
        update_in(data, ["games", Access.at(2), "seats", Access.at(0)], &Map.merge(&1, changes))

      assert {:error, _} = Imports.import_portable(Jason.encode!(bad), nil)
      assert Repo.aggregate(Game, :count) == 0
      assert Repo.aggregate(Player, :count) == 0
      assert Repo.aggregate(Printing, :count) == 0
    end
  end

  test "same-name commander conflict blocks instead of replacing destination metadata", ctx do
    {:ok, export} = Imports.export_portable()

    {:ok, _} =
      Games.update_deck(ctx.deck, %{commander_name: "Other Commander", commander_card_id: nil})

    assert {:error, message} = Imports.import_portable(Jason.encode!(export), nil)
    assert message =~ "different commanders"
    assert Games.get_deck(ctx.deck.id).commander_name == "Other Commander"
  end

  defp clear_history do
    Enum.each([SheetReceipt, GamePlayer, Game, Deck, Player, Printing, Card], &Repo.delete_all/1)
  end
end
