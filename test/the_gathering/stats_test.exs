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

    # Dashboard commanders carry the same canonical catalog ID the Commanders page uses,
    # and honour the date range.
    assert %{name: "Kangee, Sky Warden", art_crop_url: "https://cards.example/kangee-art.jpg"} =
             kangee = Enum.find(stats.commanders, &(&1.id == "kangee"))

    assert %{games: 3, wins: 2, losses: 1} = kangee
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

  test "player color records count only that player's seats and merge decks by color",
       %{players: players, decks: decks} do
    # A second Alice deck that shares Birds' colors (in another order) and a third in
    # different colors: same-color decks merge into one row, others stay separate.
    {:ok, more_birds} =
      Games.create_deck(%{
        player_id: players["Alice"].id,
        name: "More Birds",
        commander_name: "Isperia, Supreme Judge",
        color_identity: "UW"
      })

    {:ok, rats} =
      Games.create_deck(%{
        player_id: players["Alice"].id,
        name: "Rats",
        commander_name: "Marrow-Gnawer",
        color_identity: "B"
      })

    game(
      players,
      %{decks | "Alice" => more_birds},
      ~U[2026-04-01 00:00:00Z],
      "Bob",
      ~w(Alice Bob Cara)
    )

    game(
      players,
      %{decks | "Alice" => rats},
      ~U[2026-04-02 00:00:00Z],
      "Alice",
      ~w(Alice Bob Cara)
    )

    game(
      players,
      %{decks | "Alice" => rats},
      ~U[2026-04-03 00:00:00Z],
      "Cara",
      ~w(Alice Bob Cara)
    )

    stats = Stats.player(players["Alice"].id)

    assert Enum.map(stats.color_win_rates, &{&1.id, &1.name, &1.games, &1.wins, &1.win_rate}) ==
             [{"WU", "Azorius", 7, 3, 42.9}, {"B", "Mono-Black", 2, 1, 50.0}]

    # Opponents' colors never leak into a player's own breakdown, but they do count
    # for the group overview.
    refute Enum.any?(stats.color_win_rates, &(&1.id == "R"))
    assert %{games: 9} = Enum.find(Stats.overview().color_win_rates, &(&1.id == "R"))
  end

  test "overview and player views add Elo, matchups, game lengths, colors, and rivals",
       %{players: players, decks: decks} do
    # A seventh, quicker game so the fastest win is unambiguous: Bob wins in 40 minutes.
    {:ok, _quick} =
      Games.create_game(%{
        played_at: ~U[2026-04-01 12:00:00Z],
        duration_minutes: 40,
        turns: 6,
        source: "manual",
        seats:
          ~w(Bob Alice Cara)
          |> Enum.with_index(1)
          |> Enum.map(fn {name, seat} ->
            %{
              player_id: players[name].id,
              deck_id: decks[name].id,
              seat: seat,
              result: if(name == "Bob", do: "win", else: "loss")
            }
          end)
      })

    overview = Stats.overview()

    assert length(overview.game_times) == 7
    assert hd(overview.game_times) == ~U[2026-04-01 12:00:00Z]

    assert overview.game_lengths.durations == [
             %{from: 30, to: 45, games: 1},
             %{from: 45, to: 60, games: 0},
             %{from: 60, to: 75, games: 0},
             %{from: 75, to: 90, games: 5}
           ]

    assert overview.game_lengths.turns == [
             %{from: 6, to: 8, games: 1},
             %{from: 8, to: 10, games: 5}
           ]

    assert %{duration_minutes: 40, winner: %{name: "Bob"}, result: nil} =
             overview.game_lengths.fastest_win

    assert %{duration_minutes: 75} = overview.game_lengths.longest_game

    # Alice has the most wins, so she is rated highest; ratings are zero-sum up to rounding.
    assert [%{name: "Alice", games: 7} | _] = overview.elo
    assert length(hd(overview.elo).history) == 7
    assert overview.elo |> Enum.map(& &1.rating) |> Enum.sum() |> Kernel.-(3000) |> abs() <= 1

    alice_bob =
      Enum.find(
        overview.matchups,
        &(&1.id == players["Alice"].id and &1.opponent_id == players["Bob"].id)
      )

    assert %{games: 7, wins: 3, losses: 3, draws: 1, win_rate: 42.9} = alice_bob

    bob_alice =
      Enum.find(
        overview.matchups,
        &(&1.id == players["Bob"].id and &1.opponent_id == players["Alice"].id)
      )

    assert %{games: 7, wins: 2, win_rate: 28.6} = bob_alice

    player = Stats.player(players["Alice"].id)

    assert %{rank: 1, players: 3} = player.elo
    assert length(player.elo.history) == 7
    assert player.average_duration_minutes == 69.2
    assert player.average_turns == 8.5
    # Alice's own wins were all 75-minute games; the 40-minute game was Bob's win.
    assert %{duration_minutes: 75, result: "win"} = player.game_lengths.fastest_win
    assert %{duration_minutes: 75} = player.game_lengths.longest_game

    assert Enum.map(player.color_exposure, &{&1.id, &1.games, &1.wins, &1.share}) == [
             {"W", 7, 3, 100.0},
             {"U", 7, 3, 100.0},
             {"B", 0, 0, 0.0},
             {"R", 0, 0, 0.0},
             {"G", 0, 0, 0.0}
           ]

    # Krenko beat Alice twice and Lathril once; Alice beat both three times.
    assert Enum.map(player.rival_commanders, &{&1.name, &1.faced, &1.beat_me, &1.beaten}) == [
             {"Krenko, Mob Boss", 7, 2, 3},
             {"Lathril, Blade of the Elves", 7, 1, 3}
           ]

    # The commander page counts how often Kangee beat each opponent, not just their record.
    kangee = Stats.commander("kangee")
    assert %{games: 7, wins: 2, beaten: 3} = Enum.find(kangee.opponents, &(&1.name == "Bob"))
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

  test "commander stats aggregate across pilots, count partner decks for both partners, and fall back to names",
       %{players: players, decks: decks} do
    # Bob's second deck pairs the name-only Krenko with Kangee as a partner; he beats Alice's
    # Kangee deck with it once, so that game holds two Kangee seats.
    {:ok, partners} =
      Games.create_deck(%{
        player_id: players["Bob"].id,
        name: "Partners",
        commander_name: "Krenko, Mob Boss",
        partner_card_id: "kangee",
        partner_name: "Kangee, Sky Warden",
        color_identity: "WUR"
      })

    game(players, Map.put(decks, "Bob", partners), ~U[2026-04-01 12:00:00Z], "Bob", [
      "Bob",
      "Alice",
      "Cara"
    ])

    commanders = Stats.commanders()

    assert Enum.map(commanders, & &1.name) |> Enum.sort() == [
             "Kangee, Sky Warden",
             "Krenko, Mob Boss",
             "Lathril, Blade of the Elves"
           ]

    kangee = Enum.find(commanders, &(&1.id == "kangee"))

    assert %{
             name: "Kangee, Sky Warden",
             art_crop_url: "https://cards.example/kangee-art.jpg",
             games: 8,
             wins: 4,
             losses: 3,
             draws: 1,
             win_rate: 50.0,
             pilots: 2,
             decks: 2,
             last_played_at: ~U[2026-04-01 12:00:00Z]
           } = kangee

    # Krenko is not in the catalog, so it is addressed by name and both Bob decks group together.
    krenko = Enum.find(commanders, &(&1.name == "Krenko, Mob Boss"))

    assert %{id: "Krenko, Mob Boss", art_crop_url: nil, games: 7, wins: 2, pilots: 1, decks: 2} =
             krenko

    detail = Stats.commander("kangee")
    assert detail.commander.id == "kangee"
    assert detail.record == %{games: 8, wins: 4, losses: 3, draws: 1, win_rate: 50.0}
    assert Enum.map(detail.pilots, &{&1.name, &1.games}) == [{"Alice", 7}, {"Bob", 1}]
    assert Enum.map(detail.decks, &{&1.name, &1.games}) == [{"Birds", 7}, {"Partners", 1}]
    assert [%{name: "Krenko, Mob Boss", games: 1, wins: 1}] = detail.partners
    assert Enum.any?(detail.opponents, &(&1.name == "Cara" and &1.games == 7))
    # The mirror match counts both Kangee seats, so the trend ends at the record.
    assert length(detail.win_rate_over_time) == 7
    assert List.last(detail.win_rate_over_time).win_rate == 50.0
    assert hd(detail.recent_games).result == "win"

    by_name = Stats.commander("krenko, mob boss")
    assert by_name.record.games == 7
    assert [%{name: "Kangee, Sky Warden", games: 1}] = by_name.partners

    assert Stats.commander("kangee", %{"date_from" => "2026-04-01"}).record.games == 2
    assert Stats.commander("00000000-0000-0000-0000-000000000000") == nil
  end

  test "commander detail query resolves a name-only legacy deck" do
    detail = Stats.commander("krenko, mob boss")

    assert detail.commander == %{
             id: "Krenko, Mob Boss",
             name: "Krenko, Mob Boss",
             art_crop_url: nil,
             color_identity: nil
           }

    assert detail.record == %{games: 6, wins: 1, losses: 4, draws: 1, win_rate: 16.7}
  end

  test "commander identity is canonical across stored IDs, names, seat order, and the overview",
       %{players: players, decks: decks} do
    # Bob records Kangee by name only; Cara's deck still points at a printing the catalog
    # no longer carries. Both must fold into the catalog card `kangee`.
    {:ok, by_name} =
      Games.create_deck(%{
        player_id: players["Bob"].id,
        name: "Name-only Kangee",
        commander_name: "kangee, sky warden",
        partner_name: "Tymna the Weaver",
        color_identity: "WUB"
      })

    {:ok, old_printing} =
      Games.create_deck(%{
        player_id: players["Cara"].id,
        name: "Old Kangee",
        commander_card_id: "kangee-old-printing",
        commander_name: "Kangee, Sky Warden",
        color_identity: "WU"
      })

    mirror = %{decks | "Bob" => by_name, "Cara" => old_printing}
    # Three Kangee seats in one game: Cara wins, Alice and Bob lose.
    game(players, mirror, ~U[2026-05-01 12:00:00Z], "Cara", ["Bob", "Alice", "Cara"])

    commanders = Stats.commanders()
    kangee_rows = Enum.filter(commanders, &(&1.name == "Kangee, Sky Warden"))
    assert [%{id: "kangee", games: 9, wins: 4, decks: 3, pilots: 3}] = kangee_rows

    # Every published ID resolves to a detail page whose record matches the list row.
    for row <- commanders do
      detail = Stats.commander(row.id)
      assert detail, "#{row.name} (#{row.id}) has no detail"
      assert detail.commander.id == row.id
      assert detail.record.games == row.games
    end

    # Legacy stored IDs and names still resolve, to the same canonical commander.
    assert Stats.commander("kangee-old-printing").commander.id == "kangee"
    assert Stats.commander("Kangee, Sky Warden").record.games == 9

    detail = Stats.commander("kangee")
    assert detail.record == %{games: 9, wins: 4, losses: 4, draws: 1, win_rate: 44.4}
    assert List.last(detail.win_rate_over_time).win_rate == 44.4
    assert hd(detail.recent_games).result == "win"
    refute Enum.any?(detail.partners, &(&1.name == "Kangee, Sky Warden"))
    assert Enum.any?(detail.partners, &(&1.name == "Tymna the Weaver" and &1.games == 1))

    # The three-way mirror match is one appearance per seat regardless of seat order.
    game(players, mirror, ~U[2026-05-02 12:00:00Z], "Alice", ["Cara", "Bob", "Alice"])
    game(players, mirror, ~U[2026-05-03 12:00:00Z], "Bob", ["Alice", "Cara", "Bob"])
    reordered = Stats.commander("kangee", %{"date_from" => "2026-05-01"})
    assert reordered.record == %{games: 9, wins: 3, losses: 6, draws: 0, win_rate: 33.3}
    assert Enum.map(reordered.win_rate_over_time, & &1.win_rate) == [33.3, 33.3, 33.3]

    # The dashboard shares the same aggregation, so a partner-only commander appears there.
    overview = Stats.overview()
    assert overview.commanders == Enum.take(Stats.commanders(), 8)
    assert Enum.any?(overview.commanders, &(&1.name == "Tymna the Weaver"))
    assert Enum.find(overview.commanders, &(&1.id == "kangee")).games == 15
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
