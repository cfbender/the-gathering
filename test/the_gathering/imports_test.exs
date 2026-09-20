defmodule TheGathering.ImportsTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.AccountsFixtures
  alias TheGathering.Catalog.{Card, CardData}
  alias TheGathering.Games
  alias TheGathering.Imports
  alias TheGathering.Repo

  @csv """
  game_id,date,player,deck,commander,seat,result,mvp_card,duration_minutes,turns,notes
  friday-1,2026-09-18,Alice,Birds,"Kangee, Sky Warden",1,win,Swan Song,75,10,Close game
  friday-1,2026-09-18,Bob,Goblins,Krenko,2,loss,,75,10,Close game
  """

  test "previews the native seat-per-row template and reports matches and creates" do
    {:ok, alice} = Games.create_player(%{name: "Alice"})

    {:ok, birds} =
      Games.create_deck(%{
        player_id: alice.id,
        name: "Birds",
        commander_name: "Kangee, Sky Warden"
      })

    preview = Imports.preview_csv(@csv)

    assert preview.valid
    assert preview.errors == []
    assert [%{game_id: "friday-1", played_at: ~U[2026-09-18 12:00:00Z]} = game] = preview.games

    assert Enum.map(game.seats, &{&1.line, &1.player, &1.result}) == [
             {2, "Alice", "win"},
             {3, "Bob", "loss"}
           ]

    assert preview.players == %{create: ["Bob"], matched: [%{id: alice.id, name: "Alice"}]}

    assert preview.decks.matched == [
             %{
               id: birds.id,
               player_id: alice.id,
               player: "Alice",
               name: "Birds",
               commander: "Kangee, Sky Warden"
             }
           ]

    assert preview.decks.create == [%{player: "Bob", name: "Goblins", commander: "Krenko"}]
  end

  test "reports invalid rows with CSV line numbers" do
    csv = String.replace(@csv, "Goblins,Krenko,2,loss", "Goblins,Krenko,nope,victory")
    preview = Imports.preview_csv(csv)

    refute preview.valid
    assert %{line: 3, field: "seat", message: "must be a positive integer"} in preview.errors
    assert %{line: 3, field: "result", message: "must be win, loss, or draw"} in preview.errors
  end

  test "imports players, decks, and games and skips the same normalized game on re-import" do
    user = AccountsFixtures.user_fixture()

    assert {:ok, %{created: 1, skipped: 0, game_ids: [game_id]}} =
             Imports.import_csv(@csv, user.id)

    assert {:ok, %{created: 0, skipped: 1, game_ids: [^game_id]}} =
             Imports.import_csv(@csv, user.id)

    game = Games.get_game!(game_id)
    assert game.source == "csv"
    assert game.created_by_user_id == user.id

    assert Enum.map(game.seats, &{&1.player.name, &1.deck.name, &1.mvp_card_name}) == [
             {"Alice", "Birds", "Swan Song"},
             {"Bob", "Goblins", nil}
           ]
  end

  test "commit links only imported rows instead of running global repair" do
    user = AccountsFixtures.user_fixture()
    insert_card("kangee", "Kangee, Sky Warden", ["W", "U"])
    insert_card("krenko", "Krenko", ["R"])
    insert_card("swan-song", "Swan Song", ["U"], false)

    {:ok, unrelated_player} = Games.create_player(%{name: "Unrelated"})

    {:ok, unrelated_deck} =
      Games.create_deck(%{
        player_id: unrelated_player.id,
        name: "Old deck",
        commander_name: "Kangee, Sky Warden"
      })

    assert {:ok, %{game_ids: [game_id]}} = Imports.import_csv(@csv, user.id)
    game = Games.get_game!(game_id)
    alice = Enum.find(game.seats, &(&1.player.name == "Alice"))
    bob = Enum.find(game.seats, &(&1.player.name == "Bob"))

    assert alice.deck.commander_card_id == "kangee"
    assert alice.deck.color_identity == "WU"
    assert alice.mvp_card_id == "swan-song"
    assert bob.deck.commander_card_id == "krenko"
    assert Games.get_deck!(unrelated_deck.id).commander_card_id == nil
  end

  test "rejects two winners without persisting any part of the file" do
    invalid = String.replace(@csv, "Bob,Goblins,Krenko,2,loss", "Bob,Goblins,Krenko,2,win")

    assert {:error, {:validation, preview}} = Imports.import_csv(invalid, 42)
    refute preview.valid
    assert Enum.all?(preview.errors, &(&1.field == "result"))
    assert Games.list_players() == []
    assert Games.list_decks() == []
    assert {[], _pagination} = Games.list_games()
  end

  test "accepts the official Mythic Track spreadsheet headers" do
    csv = """
    Date,Format,Playgroup,GameName,Bracket,Platform,Player1,Player2,Player3,Player4,Player1Commander,Player2Commander,Player3Commander,Player4Commander,Player1Mulligans,Player2Mulligans,Player3Mulligans,Player4Mulligans,Winner,GameTimeMinutes,TotalTurns,WinCondition,Tags,Notes
    1/2/2024,1,Friends,Game,3,1,Alice,Bob,,,Kangee,Krenko,,,,,,,Bob,88,8,11,,Imported
    """

    preview = Imports.preview_csv(csv)

    assert preview.valid
    assert [%{duration_minutes: 88, turns: 8, notes: "Imported", seats: seats}] = preview.games

    assert Enum.map(seats, &{&1.player, &1.deck, &1.result}) == [
             {"Alice", "Kangee", "loss"},
             {"Bob", "Krenko", "win"}
           ]
  end

  defp insert_card(id, name, colors, commander \\ true) do
    Repo.insert!(%Card{
      id: id,
      oracle_id: "oracle-#{id}",
      name: name,
      normalized_name: CardData.normalize_name(name),
      cmc: 0.0,
      type_line: if(commander, do: "Legendary Creature", else: "Instant"),
      colors: colors,
      color_identity: colors,
      image_uris: %{},
      set_code: "tst",
      collector_number: id,
      layout: "normal",
      rarity: "rare",
      released_at: ~D[2024-01-01],
      commander_legal: true,
      can_be_commander: commander
    })
  end
end
