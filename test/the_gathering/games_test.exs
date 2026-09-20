defmodule TheGathering.GamesTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.Accounts.User
  alias TheGathering.AccountsFixtures
  alias TheGathering.Games

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
