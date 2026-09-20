defmodule TheGathering.StatsTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.{Accounts, Games, Repo, Stats}
  alias TheGathering.Catalog.Card

  setup do
    Repo.insert!(%Card{
      id: "kangee",
      oracle_id: "oracle-kangee",
      name: "Kangee, Sky Warden",
      normalized_name: "kangee, sky warden",
      cmc: 0.0,
      type_line: "Legendary Creature",
      colors: [],
      color_identity: [],
      image_uris: %{"art_crop" => "https://cards.example/kangee-art.jpg"},
      set_code: "tst",
      collector_number: "1",
      layout: "normal",
      rarity: "rare",
      commander_legal: true,
      can_be_commander: true
    })

    Repo.insert!(%Card{
      id: "swords",
      oracle_id: "oracle-swords",
      name: "Swords to Plowshares",
      normalized_name: "swords to plowshares",
      cmc: 1.0,
      type_line: "Instant",
      colors: [],
      color_identity: [],
      image_uris: %{"art_crop" => "https://cards.example/swords-art.jpg"},
      set_code: "tst",
      collector_number: "2",
      layout: "normal",
      rarity: "rare",
      commander_legal: true,
      can_be_commander: false
    })

    players =
      for name <- ~w(Alice Bob Cara), into: %{} do
        {:ok, player} = Games.create_player(%{name: name})
        {name, player}
      end

    {:ok, birds} =
      Games.create_deck(%{
        player_id: players["Alice"].id,
        name: "Birds",
        commander_card_id: "kangee",
        commander_name: "Kangee, Sky Warden",
        color_identity: "WU"
      })

    {:ok, goblins} =
      Games.create_deck(%{
        player_id: players["Bob"].id,
        name: "Goblins",
        commander_name: "Krenko, Mob Boss",
        color_identity: "R"
      })

    {:ok, elves} =
      Games.create_deck(%{
        player_id: players["Cara"].id,
        name: "Elves",
        commander_name: "Lathril, Blade of the Elves",
        color_identity: "BG"
      })

    decks = %{"Alice" => birds, "Bob" => goblins, "Cara" => elves}

    game(players, decks, ~U[2026-01-01 00:00:00Z], "Alice", ["Alice", "Bob", "Cara"])
    game(players, decks, ~U[2026-01-15 12:00:00Z], "Bob", ["Bob", "Cara", "Alice"])
    game(players, decks, ~U[2026-02-01 23:59:59Z], "Alice", ["Cara", "Alice", "Bob"])
    game(players, decks, ~U[2026-02-12 12:00:00Z], "Alice", ["Alice", "Cara", "Bob"])
    game(players, decks, ~U[2026-03-01 00:00:00Z], "Cara", ["Bob", "Alice", "Cara"])
    draw(players, decks, ~U[2026-03-05 12:00:00Z])

    %{players: players, decks: decks}
  end

  test "overview reports asymmetric records, draws, seats, colors, and inclusive boundaries", %{
    players: players
  } do
    stats = Stats.overview(%{"date_from" => "2026-01-01", "date_to" => "2026-02-01"})

    assert stats.games_count == 3
    assert stats.games_by_month == [%{month: "2026-01", games: 2}, %{month: "2026-02", games: 1}]

    alice = Enum.find(stats.leaderboard, &(&1.id == players["Alice"].id))

    assert alice == %{
             id: players["Alice"].id,
             name: "Alice",
             games: 3,
             wins: 2,
             losses: 1,
             draws: 0,
             win_rate: 66.7
           }

    seat_two = Enum.find(stats.seat_win_rates, &(&1.id == 2))
    assert %{games: 3, wins: 1, win_rate: 33.3} = seat_two

    wu = Enum.find(stats.color_win_rates, &(&1.id == "WU"))
    assert %{name: "Azorius", games: 3, wins: 2, win_rate: 66.7} = wu

    assert %{id: "Kangee, Sky Warden", art_crop_url: "https://cards.example/kangee-art.jpg"} =
             Enum.find(stats.commanders, &(&1.id == "Kangee, Sky Warden"))
  end

  test "player stats compute ordered current and longest streaks plus head-to-head", %{
    players: players
  } do
    stats = Stats.player(players["Alice"].id)

    assert stats.record == %{games: 6, wins: 3, losses: 2, draws: 1, win_rate: 50.0}
    assert stats.streaks == %{current_wins: 0, longest_wins: 2}
    assert stats.recent_form == ~w(draw loss win win loss win)
    assert stats.favorite_seat == 1
    assert stats.best_seat == 1

    bob = Enum.find(stats.head_to_head, &(&1.id == players["Bob"].id))
    assert %{games: 6, wins: 3, losses: 1, draws: 1} = bob
  end

  test "deck stats include record, opponents, averages, and recent results", %{decks: decks} do
    stats = Stats.deck(decks["Alice"].id)

    assert stats.record == %{games: 6, wins: 3, losses: 2, draws: 1, win_rate: 50.0}
    assert stats.average_duration_minutes == 75.0
    assert stats.average_turns == 9.0
    assert Enum.map(stats.recent_games, & &1.result) == ~w(draw loss win win loss win)
    assert Enum.any?(stats.opponents, &(&1.name == "Bob" and &1.games == 6))
  end

  test "the detailed-stats cutoff keeps records but drops earlier seat, timing, and MVP data",
       %{players: players, decks: decks} do
    # Inclusive: the 2026-02-12 game counts, the 2026-02-01 game does not.
    {:ok, _} = Accounts.update_settings(%{detailed_stats_from: ~D[2026-02-12]})

    overview = Stats.overview()
    assert overview.detailed_stats_from == ~D[2026-02-12]
    assert overview.games_count == 6
    alice = Enum.find(overview.leaderboard, &(&1.id == players["Alice"].id))
    assert %{games: 6, wins: 3} = alice
    # All six games seat 1 would be 3 wins of 6; only the three later games count here.
    assert %{games: 3, wins: 1, win_rate: 33.3} =
             Enum.find(overview.seat_win_rates, &(&1.id == 1))

    player = Stats.player(players["Alice"].id)
    assert player.record == %{games: 6, wins: 3, losses: 2, draws: 1, win_rate: 50.0}
    assert player.streaks == %{current_wins: 0, longest_wins: 2}
    assert %{games: 2, wins: 1} = Enum.find(player.seat_win_rates, &(&1.id == 1))
    assert player.favorite_seat == 1

    assert [
             %{
               name: "Swords to Plowshares",
               mentions: 1,
               art_crop_url: "https://cards.example/swords-art.jpg"
             }
           ] = player.mvp_cards

    # Move the cutoff past every timed game: the record stays, the averages disappear.
    {:ok, _} = Accounts.update_settings(%{detailed_stats_from: ~D[2026-03-05]})
    deck = Stats.deck(decks["Alice"].id)
    assert deck.record.games == 6
    assert deck.average_duration_minutes == nil
    assert deck.average_turns == nil
    assert Stats.player(players["Alice"].id).mvp_cards == []

    # Clearing the setting restores every figure.
    {:ok, _} = Accounts.update_settings(%{detailed_stats_from: ""})
    assert Stats.overview().detailed_stats_from == nil
    assert Stats.deck(decks["Alice"].id).average_turns == 9.0
  end

  defp game(players, decks, played_at, winner, order) do
    seats =
      order
      |> Enum.with_index(1)
      |> Enum.map(fn {name, seat} ->
        %{
          player_id: players[name].id,
          deck_id: decks[name].id,
          seat: seat,
          result: if(name == winner, do: "win", else: "loss"),
          mvp_card_id: if(name == "Alice" and winner == "Alice", do: "swords"),
          mvp_card_name: if(name == "Alice" and winner == "Alice", do: "Swords to Plowshares")
        }
      end)

    {:ok, _game} =
      Games.create_game(%{
        played_at: played_at,
        duration_minutes: 75,
        turns: 9,
        source: "manual",
        seats: seats
      })
  end

  defp draw(players, decks, played_at) do
    seats =
      ~w(Alice Bob Cara)
      |> Enum.with_index(1)
      |> Enum.map(fn {name, seat} ->
        %{player_id: players[name].id, deck_id: decks[name].id, seat: seat, result: "draw"}
      end)

    {:ok, _game} = Games.create_game(%{played_at: played_at, source: "manual", seats: seats})
  end
end
