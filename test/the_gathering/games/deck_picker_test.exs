defmodule TheGathering.Games.DeckPickerTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.AccountsFixtures
  alias TheGathering.Games
  alias TheGathering.Games.{Deck, DeckPicker}
  alias TheGathering.Repo

  setup do
    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: "Chooser"}, user.id)
    %{user: user, player: player}
  end

  test "weights favor never-played, older, skipped, and less-played decks" do
    now = ~U[2026-09-20 12:00:00Z]

    candidates = [
      candidate("Recent", 0, 1, ~U[2026-09-20 11:00:00Z]),
      candidate("Old", 0, 1, ~U[2026-08-20 12:00:00Z]),
      candidate("Skipped", 2, 1, ~U[2026-09-20 11:00:00Z]),
      candidate("Frequent", 0, 10, ~U[2026-09-20 11:00:00Z]),
      candidate("Never", 0, 0, nil)
    ]

    weights = DeckPicker.selection_weights(candidates, now) |> Map.new(&{&1.deck.name, &1.weight})

    assert weights["Never"] > weights["Old"]
    assert weights["Old"] > weights["Recent"]
    assert weights["Skipped"] > weights["Recent"]
    assert weights["Recent"] > weights["Frequent"]
  end

  test "excluded and archived decks never appear", %{user: user, player: player} do
    {:ok, excluded} = deck(player, "Excluded")
    {:ok, archived} = deck(player, "Archived", archived_at: ~U[2026-09-20 12:00:00Z])
    {:ok, eligible} = deck(player, "Eligible")

    Repo.update_all(from(item in Deck, where: item.id == ^excluded.id),
      set: [included_for_play: false]
    )

    assert {:ok, %{deck: picked}} = Games.pick_deck(user, random: fn -> 0.0 end)
    assert picked.id == eligible.id
    refute picked.id in [excluded.id, archived.id]
  end

  test "skip increments and choose clears the skip count", %{user: user, player: player} do
    {:ok, deck} = deck(player, "Krenko")

    assert {:ok, skipped} = Games.record_deck_outcome(user, deck.id, :skipped)
    assert skipped.skip_count == 1
    assert {:ok, skipped} = Games.record_deck_outcome(user, deck.id, :skipped)
    assert skipped.skip_count == 2

    assert {:ok, played} = Games.record_deck_outcome(user, deck.id, :played)
    assert played.skip_count == 0
  end

  test "play count and last played are derived from game seats", %{user: user, player: player} do
    {:ok, deck} = deck(player, "Birds")
    {:ok, opponent} = Games.create_player(%{name: "Opponent"})

    assert {:ok, _game} =
             Games.create_game(%{
               played_at: ~U[2026-09-19 18:00:00Z],
               seats: [
                 %{player_id: player.id, deck_id: deck.id, seat: 1, result: "win"},
                 %{player_id: opponent.id, seat: 2, result: "loss"}
               ]
             })

    assert {:ok, pick} = Games.pick_deck(user, random: fn -> 0.0 end)
    assert pick.play_count == 1
    assert pick.last_played_at == ~U[2026-09-19 18:00:00Z]
  end

  defp candidate(name, skip_count, play_count, last_played_at) do
    %{
      deck: %Deck{name: name, skip_count: skip_count},
      play_count: play_count,
      last_played_at: last_played_at
    }
  end

  defp deck(player, name, attrs \\ []) do
    Games.create_deck(
      attrs
      |> Map.new()
      |> Map.merge(%{player_id: player.id, name: name, commander_name: name})
    )
  end
end
