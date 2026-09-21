defmodule TheGathering.Imports.SheetReconciliationTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.{Games, Imports, Repo}
  alias TheGathering.Imports.SheetReceipt

  @header "Date\tWinner\tDeck\tDan\tMatt\tJesse\tWin Con\tOther Decks\tNotes\n"
  @row "3/17/25\tDaniel\tEdgar Markov\t1\t1\t\tSwing Out\tReality (Kenrith); Matt (Sergeant John Benton)\tCorrected history\n"

  setup do
    players =
      Map.new(~w(Daniel Reality Matt Jesse Drew Landon), fn name ->
        {:ok, player} = Games.create_player(%{name: name})
        {name, player}
      end)

    {:ok, deck} =
      Games.create_deck(%{
        player_id: players["Daniel"].id,
        name: "Cleaned-up vampires",
        commander_name: "Edgar Markov"
      })

    # Deliberately different turn order, deck names, results, notes, and kill counts.
    {:ok, game} =
      Games.create_game(%{
        played_at: ~U[2025-03-18 01:30:00Z],
        source: "mythic_track",
        external_id: "original",
        notes: "Old notes",
        turns: 8,
        duration_minutes: 90,
        seats: [
          %{player_id: players["Matt"].id, seat: 1, result: "win", kills: 2, notes: "Seat note"},
          %{
            player_id: players["Daniel"].id,
            deck_id: deck.id,
            seat: 2,
            result: "loss",
            mvp_card_name: "Sol Ring"
          },
          %{player_id: players["Reality"].id, seat: 3, result: "loss", kills: 0}
        ]
      })

    %{players: players, game: game, deck: deck}
  end

  test "updates in place, preserves cleaned identity and fields, and remembers reconciled rows",
       ctx do
    params = input(ctx)
    {:ok, initial} = Imports.preview_sheet(params)
    assert hd(initial.rows).action == ctx.game.id
    assert initial.valid
    assert hd(initial.rows).status == "changed"
    assert hd(initial.rows).match_reason =~ "nearby date"

    assert hd(initial.rows).changes == [
             %{field: "result", player: "Daniel", before: "loss", after: "win"},
             %{field: "kills", player: "Daniel", before: nil, after: 1},
             %{field: "result", player: "Matt", before: "win", after: "loss"},
             %{field: "kills", player: "Matt", before: 2, after: 1},
             %{
               field: "notes",
               player: nil,
               before: "Old notes",
               after: "Win con: Swing Out\nCorrected history"
             }
           ]

    {:ok, preview} = Imports.preview_sheet(params)
    assert preview.valid, inspect(hd(preview.rows).errors)
    assert {:ok, %{updated: 1, created: 0}} = Imports.import_sheet(params, preview.revision, nil)

    saved = Games.get_game!(ctx.game.id)

    assert {saved.source, saved.external_id, saved.played_at, saved.turns, saved.duration_minutes} ==
             {"mythic_track", "original", ~U[2025-03-18 01:30:00Z], 8, 90}

    assert saved.notes == "Win con: Swing Out\nCorrected history"

    assert Enum.map(saved.seats, &{&1.id, &1.player_id, &1.deck_id, &1.seat}) ==
             Enum.map(ctx.game.seats, &{&1.id, &1.player_id, &1.deck_id, &1.seat})

    assert Enum.map(saved.seats, &{&1.player.name, &1.result, &1.kills}) ==
             [{"Matt", "loss", 1}, {"Daniel", "win", 1}, {"Reality", "loss", 0}]

    assert Enum.at(saved.seats, 0).notes == "Seat note"
    assert Enum.at(saved.seats, 1).mvp_card_name == "Sol Ring"
    {:ok, repeated} = Imports.preview_sheet(params)
    assert hd(repeated.rows).imported_id == ctx.game.id
    assert hd(repeated.rows).action == "skip"
    assert Repo.aggregate(TheGathering.Games.Game, :count) == 1
  end

  test "blank kills and explicit zero replace counts, while blank notes are preserved",
       ctx do
    text = @header <> "3/17/25\tDaniel\tEdgar\t0\t\t\t\tReality (Kenrith); Matt (Benton)\t\n"
    params = input(ctx) |> Map.put("text", text) |> select_all(ctx.game.id)
    {:ok, preview} = Imports.preview_sheet(params)
    assert {:ok, _} = Imports.import_sheet(params, preview.revision, nil)
    game = Games.get_game!(ctx.game.id)
    assert game.notes == "Old notes"
    assert Enum.map(game.seats, & &1.kills) == [0, 0, 0]
  end

  test "deck diffs reflect committed mappings and do not invent changes when creation reuses a deck",
       ctx do
    key = Jason.encode!(["Daniel", "Edgar Markov"])
    params = input(ctx) |> Map.put("decks", %{key => "new"})
    {:ok, preview} = Imports.preview_sheet(params)
    refute Enum.any?(hd(preview.rows).changes, &(&1.field == "deck"))
    assert hd(hd(preview.rows).seats).deck_id == ctx.deck.id

    {:ok, replacement} =
      Games.create_deck(%{
        player_id: ctx.players["Daniel"].id,
        name: "Replacement",
        commander_name: "Voja"
      })

    params = input(ctx) |> Map.put("decks", %{key => replacement.id})
    {:ok, preview} = Imports.preview_sheet(params)

    assert %{field: "deck", player: "Daniel", before: "Cleaned-up vampires", after: "Replacement"} in hd(
             preview.rows
           ).changes

    assert {:ok, _} = Imports.import_sheet(params, preview.revision, nil)

    assert Games.get_game!(ctx.game.id).seats
           |> Enum.find(&(&1.player_id == ctx.players["Daniel"].id))
           |> Map.fetch!(:deck_id) == replacement.id
  end

  test "unchanged values are skipped despite sheet nicknames and different turn order", ctx do
    seats =
      Enum.map(ctx.game.seats, fn seat ->
        %{
          id: seat.id,
          player_id: seat.player_id,
          seat: seat.seat,
          deck_id: seat.deck_id,
          result: if(seat.player_id == ctx.players["Daniel"].id, do: "win", else: "loss"),
          kills: if(seat.player_id == ctx.players["Reality"].id, do: 0, else: 1)
        }
      end)

    {:ok, _} =
      Games.update_game(ctx.game, %{seats: seats, notes: "Win con: Swing Out\nCorrected history"})

    {:ok, preview} = Imports.preview_sheet(input(ctx))
    [row] = preview.rows
    assert row.target.id == ctx.game.id
    assert row.changes == []
    assert row.status == "unchanged"
    assert row.action == "skip"
    refute preview.valid
  end

  test "multiple games use deck evidence, never the recorded winner, to break ties", ctx do
    {:ok, other_deck} =
      Games.create_deck(%{
        player_id: ctx.players["Daniel"].id,
        name: "Other deck",
        commander_name: "Voja, Jaws of the Conclave"
      })

    {:ok, other} =
      Games.create_game(%{
        played_at: ctx.game.played_at,
        seats: [
          %{player_id: ctx.players["Daniel"].id, deck_id: other_deck.id, seat: 1, result: "win"},
          %{player_id: ctx.players["Matt"].id, seat: 2, result: "loss"},
          %{player_id: ctx.players["Reality"].id, seat: 3, result: "loss"}
        ]
      })

    {:ok, preview} = Imports.preview_sheet(input(ctx))
    assert hd(preview.rows).action == ctx.game.id
    assert hd(preview.rows).match_reason =~ "decks"

    params =
      input(ctx) |> Map.put("text", @header <> String.replace(@row, "Edgar Markov", "Voja"))

    {:ok, preview} = Imports.preview_sheet(params)
    assert hd(preview.rows).action == other.id

    params =
      input(ctx) |> Map.put("text", @header <> String.replace(@row, "Edgar Markov", "Nickname"))

    {:ok, preview} = Imports.preview_sheet(params)
    assert hd(preview.rows).target == nil
    assert hd(preview.rows).status == "review"
    assert hd(preview.rows).action == "skip"
    assert hd(preview.rows).match_reason =~ "Multiple games"
  end

  test "unmapped players and invalid rows never auto-select a game", ctx do
    params = input(ctx) |> Map.put("text", @header <> String.replace(@row, "Reality", "Unknown"))
    {:ok, preview} = Imports.preview_sheet(params)
    assert hd(preview.rows).action == "skip"
    assert hd(preview.rows).target == nil

    params =
      input(ctx) |> Map.put("text", @header <> String.replace(@row, "\t1\t1\t", "\t5\t1\t"))

    {:ok, preview} = Imports.preview_sheet(params)
    assert hd(preview.rows).target.id == ctx.game.id
    assert hd(preview.rows).status == "review"
    assert hd(preview.rows).action == "skip"
  end

  test "skipping a matched row preserves its comparison but prevents writes", ctx do
    params = input(ctx) |> select_all("skip")
    {:ok, preview} = Imports.preview_sheet(params)
    assert hd(preview.rows).target.id == ctx.game.id
    assert hd(preview.rows).changes != []
    refute preview.valid
    assert {:error, _} = Imports.import_sheet(params, preview.revision, nil)
    assert Games.get_game!(ctx.game.id).notes == "Old notes"
  end

  test "competing sheet rows are left for review and cannot update one game", ctx do
    params =
      input(ctx)
      |> Map.put(
        "text",
        @header <> @row <> String.replace(@row, "Corrected history", "Second game")
      )

    {:ok, preview} = Imports.preview_sheet(params)
    assert Enum.all?(preview.rows, &(&1.action == "skip"))
    params = select_all(params, ctx.game.id)
    {:ok, invalid} = Imports.preview_sheet(params)
    refute invalid.valid

    assert Enum.all?(
             invalid.rows,
             &Enum.any?(&1.errors, fn error -> error =~ "Two sheet rows" end)
           )

    assert {:error, _} = Imports.import_sheet(params, invalid.revision, nil)
    assert Games.get_game!(ctx.game.id).notes == "Old notes"
  end

  test "stale notes and changed input invalidate the preview", ctx do
    params = input(ctx) |> select_all(ctx.game.id)
    {:ok, preview} = Imports.preview_sheet(params)

    assert {:error, _} =
             Imports.import_sheet(
               Map.put(params, "text", @header <> String.replace(@row, "Corrected", "Changed")),
               preview.revision,
               nil
             )

    {:ok, _} = Games.update_game(ctx.game, %{notes: "Edited after preview"})
    assert {:error, _} = Imports.import_sheet(params, preview.revision, nil)
    assert Games.get_game!(ctx.game.id).notes == "Edited after preview"
    assert Repo.aggregate(SheetReceipt, :count) == 0
  end

  test "invalid selected rows block the batch, skipped invalid rows do not", ctx do
    params =
      input(ctx)
      |> Map.put(
        "text",
        @header <> @row <> "3/7/25\tMatt\tPantyBlink\t\t3\t\tCombo\t\tMissing opponents\n"
      )

    {:ok, preview} = Imports.preview_sheet(params)
    [valid, invalid] = preview.rows
    params = Map.put(params, "actions", %{valid.key => ctx.game.id, invalid.key => "create"})
    {:ok, blocked} = Imports.preview_sheet(params)
    refute blocked.valid
    assert {:error, _} = Imports.import_sheet(params, blocked.revision, nil)
    assert Games.get_game!(ctx.game.id).notes == "Old notes"
    params = put_in(params, ["actions", invalid.key], "skip")
    {:ok, ready} = Imports.preview_sheet(params)
    assert {:ok, %{updated: 1, skipped: 1}} = Imports.import_sheet(params, ready.revision, nil)
  end

  test "approved Jesse correction creates a missing game with explicit deck mapping", ctx do
    text =
      @header <>
        "9/4/25\tJesse\tSoul of Windgrace\t\t\t3\tSwing Out\tDrew (Merieke); Matt (Cloud); Landon (Archelos)\tJesse 3 kills - craterhoof game ender\n"

    params = input(ctx) |> Map.put("text", text) |> select_all("create") |> new_decks()
    {:ok, preview} = Imports.preview_sheet(params)
    assert preview.valid

    assert {:ok, %{created: 1, game_ids: [id]}} =
             Imports.import_sheet(params, preview.revision, nil)

    game = Games.get_game!(id)
    assert hd(game.seats).player.name == "Jesse"
    assert hd(game.seats).kills == 3
    assert Enum.all?(tl(game.seats), &(&1.kills == 0))
    assert hd(game.seats).deck.name == "Soul of Windgrace"
    {:ok, replay} = Imports.preview_sheet(params)
    assert hd(replay.rows).action == "skip"
  end

  test "October draw preserves the funny note rather than inferring a loss", ctx do
    text =
      @header <>
        "10/2/25\tN/A\tN/A\t\t\t\tN/A\tDrew (Karlov); Matt (Gylwain); Reality (Kenrith); Dan (Sokrates) Landon(Oona)\t4-way Tie due to Divine Intervention. Woo. Matt lost tho.\n"

    params = input(ctx) |> Map.put("text", text) |> select_all("create") |> new_decks()
    {:ok, preview} = Imports.preview_sheet(params)
    assert preview.valid
    assert {:ok, %{game_ids: [id]}} = Imports.import_sheet(params, preview.revision, nil)
    game = Games.get_game!(id)
    assert length(game.seats) == 5
    assert Enum.all?(game.seats, &(&1.result == "draw"))
    assert game.notes == "4-way Tie due to Divine Intervention. Woo. Matt lost tho."
  end

  test "rejects alias collisions, misplaced kills and foreign-owned decks", ctx do
    collision =
      input(ctx)
      |> Map.put("text", @header <> String.replace(@row, "Reality (Kenrith)", "Dan (Tyvar)"))
      |> select_all(ctx.game.id)

    {:ok, preview} = Imports.preview_sheet(collision)
    assert Enum.any?(hd(preview.rows).errors, &String.contains?(&1, "same player twice"))

    misplaced =
      input(ctx)
      |> Map.put(
        "text",
        @header <>
          "9/4/25\tJesse\tWindgrace\t3\t\t\tCombat\tMatt (Cloud); Drew (A); Landon (B)\t\n"
      )
      |> select_all("create")

    {:ok, preview} = Imports.preview_sheet(misplaced)
    assert Enum.any?(hd(preview.rows).errors, &String.contains?(&1, "not seated"))

    params =
      input(ctx)
      |> select_all(ctx.game.id)
      |> Map.put("decks", %{Jason.encode!(["Matt", "Sergeant John Benton"]) => ctx.deck.id})

    {:ok, preview} = Imports.preview_sheet(params)
    refute preview.valid
    assert {:error, _} = Imports.import_sheet(params, preview.revision, nil)
  end

  test "a persistence failure rolls back earlier updates and new identities", ctx do
    long_name = String.duplicate("X", 101)
    bad = "3/20/25\tNew player\t#{long_name}\t\t\t\tCombat\tMatt (Cloud)\t\n"
    params = input(ctx) |> Map.put("text", @header <> @row <> bad)
    {:ok, preview} = Imports.preview_sheet(params)
    [first, second] = preview.rows

    params =
      params
      |> Map.put("actions", %{first.key => ctx.game.id, second.key => "create"})
      |> Map.put("players", %{"Dan" => ctx.players["Daniel"].id, "New player" => "new"})
      |> new_decks()

    {:ok, preview} = Imports.preview_sheet(params)
    assert preview.valid
    assert {:error, %Ecto.Changeset{}} = Imports.import_sheet(params, preview.revision, nil)
    assert Games.get_game!(ctx.game.id).notes == "Old notes"
    refute Enum.any?(Games.list_players(), &(&1.name == "New player"))
    assert Repo.aggregate(SheetReceipt, :count) == 0
  end

  defp input(ctx),
    do: %{"text" => @header <> @row, "players" => %{"Dan" => ctx.players["Daniel"].id}}

  defp select_all(params, action) do
    {:ok, preview} = Imports.preview_sheet(params)
    Map.put(params, "actions", Map.new(preview.rows, &{&1.key, action}))
  end

  defp new_decks(params) do
    {:ok, preview} = Imports.preview_sheet(params)

    Map.put(
      params,
      "decks",
      Map.new(Enum.flat_map(preview.rows, & &1.seats), &{&1.deck_key, "new"})
    )
  end
end
