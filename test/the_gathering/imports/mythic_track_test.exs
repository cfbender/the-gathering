defmodule TheGathering.Imports.MythicTrackTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.AccountsFixtures
  alias TheGathering.Games
  alias TheGathering.Games.Game
  alias TheGathering.Imports

  # Shaped like Mythic Track's `List<GameViewModel>` (camelCase System.Text.Json output).
  defp game(overrides \\ %{}) do
    Map.merge(
      %{
        "id" => "8f3a0a44-0000-4000-8000-000000000001",
        "name" => "",
        "notes" => "Close one",
        "createdOn" => "2026-03-14T19:30:15.123456",
        "gameStatus" => 3,
        "gameType" => 1,
        "totalTurns" => 9,
        "gameTimeInMinutes" => 55,
        "players" => [
          # Listed out of turn order on purpose: seats must follow turnOrder.
          seat("Drew", 2, false, "Krenko, Mob Boss", %{"discordUserId" => "200000000000000002"}),
          seat("Daniel", 1, true, "Tifa Lockhart", %{
            "deckName" => "Tifa Punches",
            "colors" => ["G"]
          }),
          seat("Kaylyn", nil, false, "Éowyn, Shieldmaiden", %{})
        ]
      },
      overrides
    )
  end

  defp seat(name, turn_order, winner, commander, extra) do
    commander_extra = Map.drop(extra, ["discordUserId"])

    %{
      "id" => Ecto.UUID.generate(),
      "player" => %{
        "id" => Ecto.UUID.generate(),
        "name" => name,
        "discordUserId" => extra["discordUserId"]
      },
      "commander" =>
        Map.merge(
          %{
            "scryfallId" => "sf-#{String.downcase(commander)}",
            "name" => commander,
            "colors" => ["R"],
            "decklistUrl" => "",
            "deckName" => nil
          },
          commander_extra
        ),
      "commanderPartner" => nil,
      "turnOrder" => turn_order,
      "mulligans" => 0,
      "isWinner" => winner
    }
  end

  defp json(games), do: Jason.encode!(games)

  test "orders seats by turn order, derives results, decks, colours, and notes" do
    preview = Imports.preview(:mythic_track, json([game()]))

    assert preview.valid
    assert preview.warnings == []
    assert [parsed] = preview.games
    assert parsed.external_id == "8f3a0a44-0000-4000-8000-000000000001"
    assert parsed.played_at == ~U[2026-03-14 19:30:15Z]
    assert parsed.turns == 9
    assert parsed.duration_minutes == 55
    assert parsed.notes == "Close one"

    assert Enum.map(parsed.seats, &{&1.seat, &1.player, &1.result, &1.deck}) == [
             {1, "Daniel", "win", "Tifa Punches"},
             {2, "Drew", "loss", "Krenko, Mob Boss"},
             {3, "Kaylyn", "loss", "Éowyn, Shieldmaiden"}
           ]

    daniel = hd(parsed.seats)
    assert daniel.commander_card_id == "sf-tifa lockhart"
    assert daniel.color_identity == "G"
    assert Enum.at(parsed.seats, 1).discord_id == "200000000000000002"
    assert preview.players.create == ["Daniel", "Drew", "Kaylyn"]
  end

  test "a game with no winner is an all-player draw and two winners is skipped" do
    draw = game(%{"players" => Enum.map(game()["players"], &Map.put(&1, "isWinner", false))})
    assert %{valid: true, games: [parsed]} = Imports.preview(:mythic_track, json([draw]))
    assert Enum.map(parsed.seats, & &1.result) == ["draw", "draw", "draw"]

    two_winners =
      game(%{
        "id" => "8f3a0a44-0000-4000-8000-000000000002",
        "name" => "Friday pod",
        "players" => Enum.map(game()["players"], &Map.put(&1, "isWinner", true))
      })

    preview = Imports.preview(:mythic_track, json([game(), two_winners]))
    # The admin cannot fix the export here, so the good game stays importable.
    assert preview.valid
    assert preview.errors == []
    assert [%{line: 2, message: message}] = preview.warnings
    assert message =~ "more than one player is marked as the winner"
    assert message =~ "Friday pod, 2026-03-14, players: Drew, Daniel, Kaylyn"
    assert length(preview.games) == 1
  end

  test "a player listed twice skips the game and names the player" do
    [drew, daniel, kaylyn] = game()["players"]
    twice = game(%{"players" => [drew, daniel, kaylyn, Map.put(daniel, "turnOrder", 4)]})

    preview = Imports.preview(:mythic_track, json([twice]))
    assert preview.valid
    assert preview.games == []
    assert [%{line: 1, message: "skipped: Daniel is listed twice (" <> _rest}] = preview.warnings
  end

  test "skips in-progress games with a warning and names partner decks" do
    partner = %{"scryfallId" => "sf-bg", "name" => "Candlekeep Sage", "colors" => ["U"]}

    players =
      game()["players"]
      |> List.update_at(1, &Map.put(&1, "commanderPartner", partner))

    in_progress = game(%{"id" => "8f3a0a44-0000-4000-8000-000000000003", "gameStatus" => 2})

    preview = Imports.preview(:mythic_track, json([game(%{"players" => players}), in_progress]))

    assert preview.valid

    assert preview.warnings == [
             %{
               line: 2,
               message: "skipped: game is in progress (2026-03-14, players: Drew, Daniel, Kaylyn)"
             }
           ]

    assert [%{seats: [daniel | _rest]}] = preview.games
    assert daniel.deck == "Tifa Punches"
    assert daniel.partner_name == "Candlekeep Sage"
    assert daniel.partner_card_id == "sf-bg"
    assert daniel.color_identity == "UG"

    # Without a Mythic Track deck name, a partner deck is "Commander / Partner".
    unnamed =
      game(%{"players" => List.update_at(players, 1, &put_in(&1, ["commander", "deckName"], ""))})

    assert [%{seats: [unnamed_daniel | _rest]}] =
             Imports.preview(:mythic_track, json([unnamed])).games

    assert unnamed_daniel.deck == "Tifa Lockhart / Candlekeep Sage"

    # Mythic Track also writes partners as "A || B (Partners)" in the commander name.
    piped =
      game()["players"]
      |> List.update_at(1, fn player ->
        player
        |> put_in(
          ["commander", "name"],
          "Frodo, Adventurous Hobbit || Sam, Loyal Attendant (Partners)"
        )
        |> put_in(["commander", "deckName"], "")
      end)

    assert [%{seats: [piped_daniel | _rest]}] =
             Imports.preview(:mythic_track, json([game(%{"players" => piped})])).games

    assert piped_daniel.commander == "Frodo, Adventurous Hobbit"
    assert piped_daniel.partner_name == "Sam, Loyal Attendant"
    assert piped_daniel.deck == "Frodo, Adventurous Hobbit / Sam, Loyal Attendant"

    solo = game(%{"players" => [Enum.at(players, 1)]})

    assert [%{line: 1, message: "skipped: needs between 2 and 6 players, has 1 (" <> _}] =
             Imports.preview(:mythic_track, json([solo])).warnings

    assert [%{line: 1, field: "id"}] =
             Imports.preview(:mythic_track, json([game(%{"id" => ""})])).errors
  end

  test "rejects payloads that are not a game array" do
    assert [%{line: 1, field: "json"}] = Imports.preview(:mythic_track, "{\"nope\": 1}").errors
    assert [%{line: 1, field: "json"}] = Imports.preview(:mythic_track, "not json").errors
    assert [%{line: 1, field: "json"}] = Imports.preview(:mythic_track, "[]").errors
  end

  test "imports with Scryfall IDs, merges Discord identities, and skips the same GUID" do
    user = AccountsFixtures.user_fixture()

    # Drew already exists from the Discord bot under a different display name.
    {:ok, drew} = Games.find_or_create_player_by_discord_id("200000000000000002", "waxpoetik")
    {:ok, other_daniel} = Games.create_player(%{name: "daniel"})

    payload = json([game()])
    preview = Imports.preview(:mythic_track, payload)
    assert preview.players.create == ["Kaylyn"]

    assert Enum.map(preview.players.matched, & &1.id) |> Enum.sort() ==
             Enum.sort([drew.id, other_daniel.id])

    assert {:ok, %{created: 1, skipped: 0, game_ids: [game_id]}} =
             Imports.import(:mythic_track, payload, user.id)

    imported = Games.get_game!(game_id)
    assert imported.source == "mythic_track"
    assert imported.external_id == "8f3a0a44-0000-4000-8000-000000000001"

    seats = Enum.sort_by(imported.seats, & &1.seat)
    assert Enum.map(seats, & &1.player.name) == ["daniel", "waxpoetik", "Kaylyn"]
    assert Enum.map(seats, & &1.result) == ["win", "loss", "loss"]

    tifa = hd(seats).deck
    assert tifa.name == "Tifa Punches"
    assert tifa.commander_card_id == "sf-tifa lockhart"
    assert tifa.color_identity == "G"
    refute is_nil(Repo.get_by(TheGathering.Games.Player, discord_id: "200000000000000002"))

    assert {:ok, %{created: 0, skipped: 1, game_ids: [^game_id]}} =
             Imports.import(:mythic_track, payload, user.id)

    assert Repo.aggregate(Game, :count) == 1
  end

  test "commit preserves partner name, card ID, and combined colors" do
    user = AccountsFixtures.user_fixture()

    partner = %{
      "scryfallId" => "sf-candlekeep",
      "name" => "Candlekeep Sage",
      "colors" => ["U"]
    }

    players =
      game()["players"]
      |> List.update_at(1, &Map.put(&1, "commanderPartner", partner))

    assert {:ok, %{game_ids: [game_id]}} =
             Imports.import(:mythic_track, json([game(%{"players" => players})]), user.id)

    winner =
      game_id |> Games.get_game!() |> Map.fetch!(:seats) |> Enum.find(&(&1.result == "win"))

    assert winner.deck.partner_name == "Candlekeep Sage"
    assert winner.deck.partner_card_id == "sf-candlekeep"
    assert winner.deck.color_identity == "UG"
  end

  test "preview and import create a distinct player for a conflicting Discord identity" do
    user = AccountsFixtures.user_fixture()
    {:ok, existing_alice} = Games.create_player(%{name: "Alice", discord_id: "discord-alice-a"})

    players =
      game()["players"]
      |> List.update_at(0, fn player ->
        player
        |> put_in(["player", "name"], "Alice")
        |> put_in(["player", "discordUserId"], "discord-alice-b")
      end)

    payload = json([game(%{"players" => players})])
    preview = Imports.preview(:mythic_track, payload)

    assert "Alice (2)" in preview.players.create
    refute Enum.any?(preview.players.matched, &(&1.id == existing_alice.id))

    assert {:ok, %{created: 1, game_ids: [game_id]}} =
             Imports.import(:mythic_track, payload, user.id)

    imported_alice = Repo.get_by!(TheGathering.Games.Player, discord_id: "discord-alice-b")
    assert imported_alice.name == "Alice (2)"

    assert Enum.any?(Games.get_game!(game_id).seats, &(&1.player_id == imported_alice.id))
    refute Enum.any?(Games.get_game!(game_id).seats, &(&1.player_id == existing_alice.id))
  end

  test "matching Discord identity wins when the imported display name changed" do
    user = AccountsFixtures.user_fixture()

    {:ok, existing} =
      Games.create_player(%{name: "Original Discord Name", discord_id: "200000000000000002"})

    payload = json([game()])
    preview = Imports.preview(:mythic_track, payload)

    assert Enum.any?(preview.players.matched, &(&1.id == existing.id))
    refute "Drew" in preview.players.create

    assert {:ok, %{game_ids: [game_id]}} = Imports.import(:mythic_track, payload, user.id)
    assert Enum.any?(Games.get_game!(game_id).seats, &(&1.player_id == existing.id))
  end

  test "links the first key card to the winner as MVP and keeps the rest in notes" do
    user = AccountsFixtures.user_fixture()

    key_cards = [
      %{"scryfallId" => "sf-craterhoof", "name" => "Craterhoof Behemoth", "colors" => ["G"]},
      %{"scryfallId" => nil, "name" => "Finale of Devastation", "colors" => ["G"]}
    ]

    payload = json([game(%{"keyCards" => key_cards})])
    assert %{valid: true, games: [parsed]} = Imports.preview(:mythic_track, payload)

    # Daniel (seat 1) won; the losers must not receive an MVP card.
    assert Enum.map(parsed.seats, &{&1.result, &1.mvp_card, &1.mvp_card_id}) == [
             {"win", "Craterhoof Behemoth", "sf-craterhoof"},
             {"loss", nil, nil},
             {"loss", nil, nil}
           ]

    assert parsed.notes == "Close one\nKey cards: Finale of Devastation"

    assert {:ok, %{created: 1, game_ids: [game_id]}} =
             Imports.import(:mythic_track, payload, user.id)

    winner =
      game_id |> Games.get_game!() |> Map.fetch!(:seats) |> Enum.find(&(&1.result == "win"))

    assert winner.mvp_card_name == "Craterhoof Behemoth"
    assert winner.mvp_card_id == "sf-craterhoof"

    # Without a winner there is no seat to link, so every key card stays in notes.
    draw =
      game(%{
        "id" => "8f3a0a44-0000-4000-8000-000000000009",
        "keyCards" => key_cards,
        "players" => Enum.map(game()["players"], &Map.put(&1, "isWinner", false))
      })

    assert %{games: [drawn]} = Imports.preview(:mythic_track, json([draw]))
    assert Enum.all?(drawn.seats, &is_nil(&1.mvp_card))
    assert drawn.notes == "Close one\nKey cards: Craterhoof Behemoth, Finale of Devastation"
  end
end
