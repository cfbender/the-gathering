defmodule TheGathering.GamesTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.Accounts.User
  alias TheGathering.AccountsFixtures
  alias TheGathering.Games
  alias TheGathering.Games.Deck
  alias TheGathering.Repo

  defp player(name), do: Games.create_player(%{name: name}) |> elem(1)

  defp seats(players, winner_index \\ 0) do
    players
    |> Enum.with_index(1)
    |> Enum.map(fn {player, seat} ->
      %{
        player_id: player.id,
        seat: seat,
        result: if(seat - 1 == winner_index, do: "win", else: "loss")
      }
    end)
  end

  defp game_attrs(players, attrs \\ %{}) do
    Map.merge(
      %{played_at: ~U[2026-09-19 18:00:00Z], source: "manual", seats: seats(players)},
      attrs
    )
  end

  test "enforces the two-to-six seat bounds at both edges" do
    players = Enum.map(1..7, &player("Player #{&1}"))

    assert {:error, one} = Games.create_game(game_attrs(Enum.take(players, 1)))
    assert "must contain between 2 and 6 players" in errors_on(one).seats

    assert {:ok, game} = Games.create_game(game_attrs(Enum.take(players, 6)))
    assert length(game.seats) == 6

    assert {:error, seven} = Games.create_game(game_attrs(players))
    assert %{seats: seat_errors} = errors_on(seven)
    assert Enum.at(seat_errors, 6).seat == ["must be less than or equal to 6"]
  end

  test "rejects a duplicate player even when seat numbers differ" do
    first = player("Alice")
    second = player("Bob")
    attrs = game_attrs([first, second])
    [first_seat, second_seat] = attrs.seats

    assert {:error, changeset} =
             Games.create_game(%{
               attrs
               | seats: [first_seat, %{second_seat | player_id: first.id}]
             })

    assert "cannot contain the same player twice" in errors_on(changeset).seats
  end

  test "rejects a deck belonging to another player" do
    alice = player("Alice")
    bob = player("Bob")

    {:ok, deck} =
      Games.create_deck(%{player_id: alice.id, name: "Birds", commander_name: "Kangee"})

    [alice_seat, bob_seat] = seats([alice, bob])

    assert {:error, changeset} =
             Games.create_game(
               game_attrs([alice, bob], %{
                 seats: [alice_seat, Map.put(bob_seat, :deck_id, deck.id)]
               })
             )

    assert "contains a deck that does not belong to its player" in errors_on(changeset).seats
  end

  test "rejects two winners and accepts an all-draw game" do
    players = [player("Alice"), player("Bob"), player("Cara")]

    two_winners =
      Enum.map(seats(players), &%{&1 | result: if(&1.seat < 3, do: "win", else: "loss")})

    assert {:error, changeset} = Games.create_game(game_attrs(players, %{seats: two_winners}))
    assert "must have exactly one winner or all draws" in errors_on(changeset).seats

    draws = Enum.map(seats(players), &%{&1 | result: "draw"})
    assert {:ok, game} = Games.create_game(game_attrs(players, %{seats: draws}))
    assert Enum.all?(game.seats, &(&1.result == "draw"))
  end

  test "external IDs are idempotent within a source but independent across sources" do
    players = [player("Alice"), player("Bob")]
    attrs = game_attrs(players, %{source: "csv", external_id: "row-42"})

    assert {:ok, first} = Games.create_game(attrs)
    assert {:ok, repeated} = Games.create_game(Map.put(attrs, :notes, "ignored on replay"))
    assert first.id == repeated.id

    assert {:ok, discord} = Games.create_game(%{attrs | source: "discord"})
    refute discord.id == first.id
  end

  test "player names are unique case-insensitively and finder returns the existing player" do
    assert {:ok, alice} = Games.create_player(%{name: "Alice"})
    assert {:error, changeset} = Games.create_player(%{name: "  ALICE  "})
    assert "has already been taken" in errors_on(changeset).name
    assert {:ok, found} = Games.find_or_create_player_by_name("alice")
    assert found.id == alice.id
  end

  test "name finders fold case like SQLite, so non-ASCII names are found instead of re-inserted" do
    # SQLite's lower()/NOCASE leave É alone; Unicode downcase would turn it into é,
    # miss the row, and the insert would then hit the unique index.
    assert {:ok, eowyn} = Games.create_player(%{name: "Éowyn"})
    assert {:ok, found} = Games.find_or_create_player_by_name("Éowyn")
    assert found.id == eowyn.id

    attrs = %{commander_name: "Éowyn, Shieldmaiden"}
    assert {:ok, deck} = Games.find_or_create_deck(eowyn, "Éowyn, Shieldmaiden", attrs)
    assert {:ok, same} = Games.find_or_create_deck(eowyn, "Éowyn, Shieldmaiden", attrs)
    assert same.id == deck.id
    assert Repo.aggregate(Deck, :count) == 1

    # A collision that slips through must surface as a changeset error, not a raise.
    assert {:error, changeset} =
             Games.create_deck(%{
               player_id: eowyn.id,
               name: "Éowyn, Shieldmaiden",
               commander_name: "x"
             })

    assert "has already been taken" in errors_on(changeset).name
  end

  test "players carry the linked user's avatar, and nil when unlinked or the user has none" do
    linked_user = AccountsFixtures.user_fixture()

    {:ok, linked_user} =
      linked_user
      |> User.discord_profile_changeset(%{avatar_url: "https://cdn/av.png"})
      |> Repo.update()

    bare_user = AccountsFixtures.user_fixture()

    {:ok, linked} = Games.create_player(%{name: "Linked", user_id: linked_user.id})
    {:ok, bare} = Games.create_player(%{name: "Bare", user_id: bare_user.id})
    {:ok, orphan} = Games.create_player(%{name: "Orphan", user_id: 999_999})
    {:ok, unlinked} = Games.create_player(%{name: "Unlinked"})

    avatars = Games.list_players() |> Map.new(&{&1.id, &1.avatar_url})

    assert avatars == %{
             linked.id => "https://cdn/av.png",
             bare.id => nil,
             orphan.id => nil,
             unlinked.id => nil
           }

    assert Games.get_player!(linked.id).avatar_url == "https://cdn/av.png"
    assert Games.get_player!(orphan.id).avatar_url == nil
  end

  test "merging players moves seats and decks, collapses same-named decks, and carries identity" do
    drew = player("Drew")
    alice = player("Alice")
    {:ok, wax} = Games.create_player(%{name: "waxpoetik", discord_id: "200000000000000002"})

    {:ok, drew_krenko} = Games.find_or_create_deck(drew, "Krenko", %{commander_name: "Krenko"})
    {:ok, wax_krenko} = Games.find_or_create_deck(wax, "krenko", %{commander_name: "Krenko"})
    {:ok, wax_tifa} = Games.find_or_create_deck(wax, "Tifa", %{commander_name: "Tifa"})

    {:ok, game_a} =
      Games.create_game(
        game_attrs([drew, alice], %{
          seats: [
            %{player_id: drew.id, deck_id: drew_krenko.id, seat: 1, result: "win"},
            %{player_id: alice.id, seat: 2, result: "loss"}
          ]
        })
      )

    {:ok, game_b} =
      Games.create_game(
        game_attrs([wax, alice], %{
          external_id: "b",
          seats: [
            %{player_id: wax.id, deck_id: wax_krenko.id, seat: 1, result: "loss"},
            %{player_id: alice.id, seat: 2, result: "win"}
          ]
        })
      )

    assert {:ok, merged} = Games.merge_players(wax, drew)
    assert merged.id == drew.id
    assert merged.discord_id == "200000000000000002"
    assert Games.get_player(wax.id) == nil

    # waxpoetik's Krenko seat now points at Drew's existing Krenko deck; Tifa moved over.
    seats = Games.get_game!(game_b.id).seats
    assert [%{player_id: player_id, deck_id: deck_id}] = Enum.filter(seats, &(&1.seat == 1))
    assert {player_id, deck_id} == {drew.id, drew_krenko.id}
    assert Games.get_deck(wax_krenko.id) == nil
    assert Games.get_deck!(wax_tifa.id).player_id == drew.id
    assert length(Games.get_player!(drew.id).game_players) == 2

    assert Enum.map(Games.get_game!(game_a.id).seats, & &1.player_id) |> Enum.sort() ==
             Enum.sort([drew.id, alice.id])

    # Alice sat in both of Drew's games, so she cannot be merged into him.
    assert {:error, changeset} = Games.merge_players(alice, drew)
    assert errors_on(changeset).merge == ["both players are seated in the same game"]
    assert {:error, :bad_request} = Games.merge_players(drew, drew)
  end

  test "linking a player to an account merges the account's stub player into it" do
    user = AccountsFixtures.user_fixture()
    imported = player("Drew")
    {:ok, stub} = Games.create_player(%{name: "Drew (2)", user_id: user.id, discord_id: "42"})
    other = player("Other")
    {:ok, _game} = Games.create_game(game_attrs([stub, other]))

    assert {:ok, linked} = Games.link_player_to_user(imported, user)
    assert linked.id == imported.id
    assert linked.user_id == user.id
    assert linked.discord_id == "42"
    assert Games.get_player(stub.id) == nil
    assert length(Games.get_player!(imported.id).game_players) == 1

    # Linking again is a no-op; linking a player owned by another account fails.
    assert {:ok, %{id: id}} = Games.link_player_to_user(imported, user)
    assert id == imported.id
    other_user = AccountsFixtures.user_fixture()
    assert {:error, changeset} = Games.link_player_to_user(imported, other_user)
    assert errors_on(changeset).merge == ["players belong to different accounts"]
  end

  test "list_games combines filters, paginates, and orders newest first" do
    alice = player("Alice")
    bob = player("Bob")
    cara = player("Cara")

    {:ok, birds} =
      Games.create_deck(%{player_id: alice.id, name: "Birds", commander_name: "Kangee"})

    {:ok, old} =
      Games.create_game(
        game_attrs([alice, bob], %{
          played_at: ~U[2026-09-01 12:00:00Z],
          seats: [
            %{player_id: alice.id, deck_id: birds.id, seat: 1, result: "win"},
            %{player_id: bob.id, seat: 2, result: "loss"}
          ]
        })
      )

    {:ok, middle} =
      Games.create_game(game_attrs([alice, cara], %{played_at: ~U[2026-09-10 12:00:00Z]}))

    {:ok, newest} =
      Games.create_game(game_attrs([bob, cara], %{played_at: ~U[2026-09-18 12:00:00Z]}))

    {page_one, pagination} = Games.list_games(%{page: 1, per_page: 2})
    assert Enum.map(page_one, & &1.id) == [newest.id, middle.id]
    assert pagination == %{page: 1, per_page: 2, total: 3, total_pages: 2}

    {page_two, _pagination} = Games.list_games(%{page: 2, per_page: 2})
    assert Enum.map(page_two, & &1.id) == [old.id]

    {alice_games, _pagination} =
      Games.list_games(%{player_id: alice.id, date_from: "2026-09-05", date_to: "2026-09-15"})

    assert Enum.map(alice_games, & &1.id) == [middle.id]

    {deck_games, _pagination} = Games.list_games(%{deck_id: birds.id})
    assert Enum.map(deck_games, & &1.id) == [old.id]
  end
end
