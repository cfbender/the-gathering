defmodule TheGathering.Imports.CSVTransferTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.{Games, Imports, Repo}
  alias TheGathering.Games.{Deck, Game, GamePlayer, Player}

  setup do
    {:ok, alice} = Games.create_player(%{name: "Alice"})
    {:ok, bob} = Games.create_player(%{name: "Bob"})
    {:ok, carol} = Games.create_player(%{name: "Carol"})

    {:ok, alice_deck} =
      Games.create_deck(%{player_id: alice.id, name: "Birds", commander_name: "Kangee"})

    {:ok, bob_deck} =
      Games.create_deck(%{player_id: bob.id, name: "Goblins", commander_name: "Krenko"})

    {:ok, carol_deck} =
      Games.create_deck(%{player_id: carol.id, name: "Cats", commander_name: "Arahbo"})

    {:ok, game} =
      Games.create_game(%{
        played_at: ~U[2026-09-10 12:00:00Z],
        duration_minutes: 91,
        turns: 13,
        win_condition: "commander_damage",
        notes: "Keep this note",
        source: "mythic_track",
        external_id: "source-17",
        seats: [
          %{
            player_id: alice.id,
            deck_id: alice_deck.id,
            seat: 1,
            result: "win",
            kills: 2,
            mvp_card_name: "Swan Song",
            notes: "Alice note"
          },
          %{
            player_id: bob.id,
            deck_id: bob_deck.id,
            seat: 2,
            result: "loss",
            kills: 1,
            eliminated_turn: 9,
            eliminated_by_player_id: alice.id,
            notes: "Bob note"
          },
          %{player_id: carol.id, deck_id: carol_deck.id, seat: 3, result: "loss", kills: 0}
        ]
      })

    %{game: Games.get_game!(game.id), alice: alice, bob: bob, alice_deck: alice_deck}
  end

  test "preview is a dry run with a revision and material changes", ctx do
    csv = update_csv(ctx.game, "portable_id")
    player_count = Repo.aggregate(Player, :count)
    deck_count = Repo.aggregate(Deck, :count)

    preview = Imports.preview_csv(csv)

    assert preview.valid
    assert is_binary(preview.revision)
    assert byte_size(preview.revision) == 64
    assert [%{action: "update", target_id: target_id, changes: changes}] = preview.review
    assert target_id == ctx.game.id

    assert %{
             field: :played_at,
             player: nil,
             before: "2026-09-10T12:00:00Z",
             after: "2026-09-11T12:00:00Z"
           } in changes

    assert %{
             field: :win_condition,
             player: nil,
             before: "Commander Damage",
             after: "Combat Damage"
           } in changes

    assert %{field: :participant, player: "Carol", before: "Carol", after: nil} in changes
    assert %{field: :participant, player: "Dave", before: nil, after: "Dave"} in changes
    assert Repo.aggregate(Player, :count) == player_count
    assert Repo.aggregate(Deck, :count) == deck_count
    assert Repo.aggregate(GamePlayer, :count) == 3
  end

  test "commit preserves identity and retained seat metadata while replacing, swapping and changing a deck",
       ctx do
    csv = update_csv(ctx.game, "source")
    preview = Imports.preview_csv(csv)
    old_seats = Map.new(ctx.game.seats, &{&1.player.name, &1})

    assert {:ok, %{created: 0, updated: 1, skipped: 0, game_ids: [id]}} =
             Imports.import_csv(csv, nil, preview.revision)

    assert id == ctx.game.id
    saved = Games.get_game!(id)

    assert {saved.source, saved.external_id, saved.portable_id} ==
             {ctx.game.source, ctx.game.external_id, ctx.game.portable_id}

    assert {saved.played_at, saved.win_condition, saved.duration_minutes, saved.turns,
            saved.notes} ==
             {~U[2026-09-11 12:00:00Z], "combat_damage", 91, 13, "Keep this note"}

    seats = Map.new(saved.seats, &{&1.player.name, &1})
    assert seats["Bob"].id == old_seats["Bob"].id
    assert seats["Bob"].seat == 1

    assert {seats["Bob"].eliminated_turn, seats["Bob"].eliminated_by_player_id,
            seats["Bob"].notes} ==
             {9, ctx.alice.id, "Bob note"}

    assert seats["Alice"].id == old_seats["Alice"].id

    assert {seats["Alice"].seat, seats["Alice"].kills, seats["Alice"].mvp_card_name,
            seats["Alice"].notes} ==
             {2, 0, "Swan Song", "Alice note"}

    refute Map.has_key?(seats, "Carol")
    assert seats["Dave"].id not in Enum.map(old_seats, fn {_name, seat} -> seat.id end)
    assert seats["Alice"].deck.name == "Angels"
    assert seats["Alice"].deck.commander_name == "Giada"
    assert Repo.get!(Deck, ctx.alice_deck.id).commander_name == "Kangee"
  end

  test "repeating a reviewed update skips with no material changes", ctx do
    csv = update_csv(ctx.game, "portable_id")
    first = Imports.preview_csv(csv)
    assert {:ok, %{updated: 1}} = Imports.import_csv(csv, nil, first.revision)

    repeated = Imports.preview_csv(csv)
    assert [%{action: "skip", target_id: id, changes: []}] = repeated.review
    assert id == ctx.game.id
    before = Games.get_game!(id)

    assert {:ok, %{updated: 0, skipped: 1, game_ids: [^id]}} =
             Imports.import_csv(csv, nil, repeated.revision)

    assert Games.get_game!(id) == before
  end

  test "adding only a win condition leaves all seat records untouched", %{game: game} do
    Repo.update_all(GamePlayer, set: [updated_at: ~U[2020-01-01 00:00:00Z]])
    before = Repo.all(GamePlayer)

    rows =
      Enum.map_join(game.seats, "\n", fn seat ->
        Enum.join(
          [
            "backfill",
            DateTime.to_iso8601(game.played_at),
            seat.player.name,
            seat.deck.name,
            seat.deck.commander_name,
            seat.seat,
            seat.result,
            "infinite_combo",
            "update",
            game.source,
            game.external_id
          ],
          ","
        )
      end)

    csv =
      "game_id,date,player,deck,commander,seat,result,win_condition,action,source,external_id\n" <>
        rows

    preview = Imports.preview_csv(csv)
    assert preview.valid
    assert {:ok, %{updated: 1}} = Imports.import_csv(csv, nil, preview.revision)
    assert Repo.all(GamePlayer) == before
    assert Games.get_game!(game.id).win_condition == "infinite_combo"
  end

  test "missing, conflicting, and duplicate update targets reject atomically", ctx do
    missing = String.replace(update_csv(ctx.game, "portable_id"), ctx.game.portable_id, "")
    assert_invalid(missing, "updates require portable_id or source and external_id")

    {:ok, other} =
      Games.create_game(%{
        played_at: ~U[2026-09-01 12:00:00Z],
        source: "csv",
        external_id: "other",
        seats: [
          %{player_id: ctx.alice.id, seat: 1, result: "win"},
          %{player_id: ctx.bob.id, seat: 2, result: "loss"}
        ]
      })

    conflict = update_csv(ctx.game, "both", other.portable_id)
    assert_invalid(conflict, "Game identities refer to different existing games.")

    duplicate =
      update_csv(ctx.game, "portable_id") <> update_rows(ctx.game, "second", "portable_id")

    assert_invalid(duplicate, "Multiple CSV games target the same existing game.")
    assert Repo.aggregate(Game, :count) == 2
    assert Repo.get!(Game, ctx.game.id).played_at == ~U[2026-09-10 12:00:00Z]
    assert Repo.get_by(Player, name: "Dave") == nil
  end

  test "database and CSV changes after preview make the revision stale without partial writes",
       ctx do
    csv = update_csv(ctx.game, "portable_id")
    preview = Imports.preview_csv(csv)
    {:ok, _} = Games.update_game(ctx.game, %{notes: "Concurrent edit"})

    assert_invalid_commit(csv, preview.revision, "Preview is stale or missing")
    assert Repo.get!(Game, ctx.game.id).notes == "Concurrent edit"
    refute Repo.get_by(Player, name: "Dave")

    fresh = Imports.preview_csv(csv)
    modified = String.replace(csv, "2026-09-11", "2026-09-12")
    assert_invalid_commit(modified, fresh.revision, "Preview is stale or missing")
    assert Repo.get!(Game, ctx.game.id).played_at == ~U[2026-09-10 12:00:00Z]
    refute Repo.get_by(Player, name: "Dave")
  end

  defp update_csv(game, identity, conflicting_portable_id \\ nil) do
    header() <> update_rows(game, "reviewed", identity, conflicting_portable_id)
  end

  defp header do
    "game_id,date,player,deck,commander,seat,result,kills,win_condition,notes,action,source,external_id,portable_id\n"
  end

  defp update_rows(game, game_id, identity, conflicting_portable_id \\ nil) do
    {source, external_id, portable_id} =
      case identity do
        "source" -> {game.source, game.external_id, ""}
        "portable_id" -> {"", "", game.portable_id}
        "both" -> {game.source, game.external_id, conflicting_portable_id}
      end

    Enum.map_join(
      [
        ["Bob", "Goblins", "Krenko", 1, "loss", ""],
        ["Alice", "Angels", "Giada", 2, "win", 0],
        ["Dave", "Dragons", "Miirym", 3, "loss", 0]
      ],
      fn [player, deck, commander, seat, result, kills] ->
        Enum.join(
          [
            game_id,
            "2026-09-11",
            player,
            deck,
            commander,
            seat,
            result,
            kills,
            "combat_damage",
            "",
            "update",
            source,
            external_id,
            portable_id
          ],
          ","
        ) <> "\n"
      end
    )
  end

  defp assert_invalid(csv, message) do
    preview = Imports.preview_csv(csv)
    refute preview.valid
    assert Enum.any?(preview.errors, &String.contains?(&1.message, message))
  end

  defp assert_invalid_commit(csv, revision, message) do
    assert {:error, {:validation, preview}} = Imports.import_csv(csv, nil, revision)
    refute preview.valid
    assert Enum.any?(preview.errors, &String.contains?(&1.message, message))
  end
end
