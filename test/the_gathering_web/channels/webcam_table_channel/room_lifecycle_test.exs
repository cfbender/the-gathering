defmodule TheGatheringWeb.WebcamTableChannel.RoomLifecycleTest do
  # Room lifecycle: reloads, crashes, closing, rematches, the shared log, idle
  # cleanup and saved sessions.
  use TheGatheringWeb.WebcamTableChannelCase

  alias TheGathering.{Games, WebcamTables}
  alias TheGathering.WebcamTables.Session
  alias TheGatheringWeb.Presence

  @peer_a "00000000-0000-4000-8000-00000000000a"
  @peer_b "00000000-0000-4000-8000-00000000000b"
  @peer_c "00000000-0000-4000-8000-00000000000c"
  @peer_d "00000000-0000-4000-8000-00000000000d"
  @new_peer "00000000-0000-4000-8000-00000000a0a0"
  @after_restart "00000000-0000-4000-8000-00000000a0a1"
  @again "00000000-0000-4000-8000-00000000a0a2"
  @fresh "00000000-0000-4000-8000-00000000f0f0"

  test "reload replaces a stale channel at capacity and its DOWN cannot erase the new seat", %{
    socket: original,
    room_id: room,
    player: player
  } do
    assert_reply push(original, "update_status", %{"life" => 23, "poison" => 6}), :ok
    for index <- 2..10, do: join_seat(room, peer(index))
    Process.unlink(original.channel_pid)
    ref = Process.monitor(original.channel_pid)
    replacement = rejoin(room, player, @new_peer)
    assert_receive {:DOWN, ^ref, :process, _, _}
    sync_room(room)
    assert replacement.assigns.participant.life == 23
    assert replacement.assigns.participant.poison == 6
    assert WebcamTables.current?(room, player.id, replacement.channel_pid)
    assert length(WebcamTables.snapshot(room).seats) == 10
    assert_reply push(replacement, "update_status", %{"life" => 22}), :ok

    assert Enum.find(WebcamTables.snapshot(room).seats, &(&1.player_id == player.id)).life ==
             22
  end

  test "a room outlives its last seat, and rejoining restores the entire mid-game snapshot",
       %{
         socket: original,
         room_id: room,
         player: player,
         deck: deck
       } do
    assert_reply push(original, "choose_deck", %{"deck_id" => deck.id}), :ok

    assert_reply push(original, "update_status", %{
                   "life" => 17,
                   "poison" => 4,
                   "rad" => 9,
                   "commander_casts" => %{"Kangee" => 3},
                   "commander_damage" => %{"19" => %{"Atraxa" => 11}}
                 }),
                 :ok

    assert_reply push(original, "take_monarch", %{}), :ok
    assert_reply push(original, "turn_settings", %{"auto_randomize" => false}), :ok
    assert_reply push(original, "start_game", %{}), :ok
    assert_reply push(original, "pass_turn", %{"revision" => 1}), :ok
    assert_reply push(original, "timer", %{"action" => "pause"}), :ok

    card = %{
      "id" => "card-1",
      "ownerPeerId" => @peer_a,
      "byPlayerName" => "Alice",
      "at" => 123,
      "card" => %{
        "id" => "art-1",
        "name" => "Forest",
        "set" => "lea",
        "collector_number" => "280"
      }
    }

    assert_reply push(original, "cards", %{"type" => "card_identified", "entry" => card}), :ok
    before = WebcamTables.snapshot(room)
    pid = room_pid(room)
    disconnect(original)
    # The room keeps running with no connections.
    assert room_pid(room) == pid
    rejoined = rejoin(room, player, @after_restart)
    after_restart = WebcamTables.snapshot(room)
    assert Map.drop(after_restart.timer, [:server_now]) == Map.drop(before.timer, [:server_now])
    assert after_restart.turns == before.turns
    assert after_restart.turns.counts == %{player.id => 2}
    assert after_restart.peer_ids == [@after_restart]
    assert after_restart.monarch.holder.peer_id == @after_restart
    assert after_restart.cards == [%{card | "ownerPeerId" => @after_restart}]
    assert after_restart.auto_randomize == false

    assert Map.drop(rejoined.assigns.participant, [:peer_id]) ==
             Map.drop(hd(before.seats), [:peer_id])

    assert_reply push(rejoined, "update_status", %{"eliminated" => true}), :ok
    disconnect(rejoined)
    eliminated = rejoin(room, player, @again)
    assert eliminated.assigns.participant.eliminated
    assert eliminated.assigns.participant.life == 17
    assert [%{peer_id: @again}] = WebcamTables.snapshot(room).eliminated_seats
  end

  @tag :capture_log
  test "a crashing room stops only its own channels, which rejoin from the saved session", %{
    socket: alice,
    room_id: room,
    player: player
  } do
    other_room = Ecto.UUID.generate()
    bystander = join_player(other_room, @peer_b, "Bob")
    assert room_pid(room) != room_pid(other_room)
    assert_reply push(alice, "update_status", %{"life" => 21}), :ok

    Process.unlink(alice.channel_pid)
    channel_ref = Process.monitor(alice.channel_pid)
    Process.exit(room_pid(room), :kill)
    assert_receive {:DOWN, ^channel_ref, :process, _, {:room_down, :killed}}

    assert_reply push(bystander, "update_status", %{"life" => 30}), :ok
    assert Enum.map(WebcamTables.snapshot(other_room).seats, & &1.life) == [30]

    assert rejoin(room, player, @peer_a).assigns.participant.life == 21
  end

  test "the owner ends the table for everyone, and its seats leave instead of rejoining", %{
    socket: alice,
    room_id: room
  } do
    bob = join_player(room, @peer_b, "Bob")
    assert_reply push(alice, "start_game", %{"randomize" => false}), :ok

    assert_reply push(bob, "end_game", %{}), :error, %{
      reason: "only the room owner can change table controls"
    }

    assert_reply push(alice, "end_game", %{"extra" => true}), :error, %{
      reason: "invalid end game"
    }

    refs =
      for channel <- [alice, bob] do
        Process.unlink(channel.channel_pid)
        {channel.channel_pid, Process.monitor(channel.channel_pid)}
      end

    assert_reply push(alice, "end_game", %{}), :ok

    for {pid, ref} <- refs do
      assert_push "table_closed", %{}
      assert_receive {:DOWN, ^ref, :process, ^pid, :normal}
    end

    assert room_pid(room) == nil
    refute Enum.any?(WebcamTables.rooms(), &(&1.id == room))
    assert Session.load(room) == nil
  end

  test "a rematch resets the same room to a lobby, keeping present seats connected", %{
    socket: alice,
    room_id: room,
    player: player,
    deck: deck
  } do
    bob = join_player(room, @peer_b, "Bob")
    cara = join_player(room, @peer_c, "Cara")
    assert_reply push(alice, "choose_deck", %{"deck_id" => deck.id}), :ok
    assert_reply push(alice, "set_mode", %{"mode" => "commander"}), :ok
    assert_reply push(alice, "arrange_seats", %{"peer_ids" => [@peer_b, @peer_c, @peer_a]}), :ok
    assert_reply push(alice, "start_game", %{"randomize" => false}), :ok
    dave = join_player(room, @peer_d, "Dave")
    assert dave.assigns.participant.spectator

    # Play a little: life, counters, a turn, an elimination, the crown, a card and a roll.
    counters = %{"life" => 31, "poison" => 3, "commander_casts" => %{"Kangee" => 2}}
    assert_reply push(alice, "update_status", counters), :ok
    assert_reply push(bob, "pass_turn", %{"revision" => 1}), :ok
    assert_reply push(alice, "set_eliminated", %{"peer_id" => @peer_c, "eliminated" => true}), :ok
    assert_reply push(bob, "take_monarch", %{}), :ok

    card = %{
      "id" => Ecto.UUID.generate(),
      "ownerPeerId" => @peer_a,
      "at" => 1,
      "card" => %{"id" => "art-1", "name" => "Forest", "set" => "lea"}
    }

    assert_reply push(bob, "cards", %{"type" => "card_identified", "entry" => card}), :ok
    assert_reply push(bob, "roll", %{"kind" => "coin"}), :ok

    # Cara leaves for good, so her seat does not carry into the new lobby.
    cara_id = cara.assigns.participant.player_id
    disconnect(cara)
    %{departing: %{^cara_id => {"Cara", token}}} = :sys.get_state(room_pid(room))
    send(room_pid(room), {:departed, cara_id, token})
    sync_room(room)

    assert_reply push(bob, "rematch", %{}), :error, %{
      reason: "only the room owner can change table controls"
    }

    assert_reply push(dave, "rematch", %{}), :error, %{
      reason: "spectators cannot change the game"
    }

    assert_reply push(alice, "rematch", %{"extra" => true}), :error, %{reason: "invalid rematch"}
    assert [_card] = WebcamTables.snapshot(room).cards

    assert_reply push(alice, "rematch", %{}), :ok

    assert_broadcast "table_state", %{
      timer: %{started_at: nil, paused_at: nil, paused_ms: 0},
      peer_ids: [@peer_b, @peer_a],
      turns: %{active_player_id: nil, counts: counts, revision: 0},
      monarch: %{holder: nil},
      cards: [],
      eliminated_seats: [],
      mode: "commander",
      team_life: %{}
    }

    assert counts == %{}
    assert_broadcast "table_log", %{entries: [%{text: "Rematch: back to setup" <> _}]}
    assert [%{text: "Rematch: back to setup" <> _}] = WebcamTables.log(room)

    alice_id = player.id
    deck_id = deck.id

    snapshot = WebcamTables.snapshot(room)
    assert snapshot.owner_id == alice_id
    assert Enum.sort(Enum.map(snapshot.seats, & &1.peer_id)) == [@peer_a, @peer_b]

    assert %{life: 40, poison: 0, commander_casts: %{}, eliminated: false, deck_id: ^deck_id} =
             Enum.find(snapshot.seats, &(&1.player_id == alice_id))

    # Each seated connection adopts its reset seat: presence, assigns and the client push.
    assert_push "seat_reset", %{participant: %{peer_id: @peer_a, life: 40, poison: 0}}
    assert_push "seat_reset", %{participant: %{peer_id: @peer_b, life: 40}}
    _ = :sys.get_state(alice.channel_pid)

    assert %{metas: [%{life: 40, poison: 0, deck_id: ^deck_id}]} =
             Presence.get_by_key(alice.topic, @peer_a)

    # A later status change builds on the reset seat, not the old game's.
    assert_reply push(alice, "update_status", %{"rad" => 1}), :ok

    assert %{life: 40, poison: 0, rad: 1} =
             Enum.find(WebcamTables.snapshot(room).seats, &(&1.player_id == alice_id))

    # Nobody was disconnected, spectators still watch, and the new lobby starts like any other.
    refute_push "table_closed", _
    assert_reply push(bob, "update_status", %{"life" => 38}), :ok
    assert_reply push(dave, "update_status", %{"life" => 7}), :error
    assert_reply push(alice, "start_game", %{"randomize" => false}), :ok
    assert_broadcast "seat_order", %{peer_ids: [@peer_b, @peer_a], shuffled: false}
  end

  @tag :capture_log
  test "the shared log records seat changes and rolls, and survives reloads and room crashes", %{
    socket: original,
    room_id: room,
    player: player
  } do
    assert_push "table_log", %{entries: [%{id: 1, text: "Alice joined the table"}]}
    assert_reply push(original, "update_status", %{"life" => 37}), :ok
    assert_broadcast "log_entry", %{id: 2, text: "Alice: 40 → 37 life"}
    assert_reply push(original, "update_status", %{"life" => 35}), :ok
    assert_broadcast "log_entry", %{id: 2, text: "Alice: 40 → 35 life", count: 2}
    assert_reply push(original, "roll", %{"kind" => "coin"}), :ok
    assert_broadcast "log_entry", %{id: 3, text: "Alice flipped a coin: " <> _}

    # A reload within the grace period logs neither a leave nor a join.
    disconnect(original)
    reloaded = rejoin(room, player, @new_peer)
    assert_push "table_log", %{entries: entries}

    assert Enum.map(entries, & &1.text) |> tl() == [
             "Alice: 40 → 35 life",
             "Alice joined the table"
           ]

    Process.unlink(reloaded.channel_pid)
    channel_ref = Process.monitor(reloaded.channel_pid)
    Process.exit(room_pid(room), :kill)
    assert_receive {:DOWN, ^channel_ref, :process, _, {:room_down, :killed}}

    rejoin(room, player, @after_restart)
    assert_push "table_log", %{entries: [%{text: "Alice joined the table"} | restored]}
    assert restored == entries
  end

  test "a merge into an older log entry is broadcast in place", %{socket: alice, room_id: room} do
    bob = join_player(room, @peer_b, "Bob")
    assert_reply push(alice, "update_status", %{"life" => 37}), :ok
    assert_broadcast "log_entry", %{id: 3, text: "Alice: 40 → 37 life"}
    assert_reply push(bob, "update_status", %{"life" => 38}), :ok
    assert_broadcast "log_entry", %{id: 4, text: "Bob: 40 → 38 life"}
    assert_reply push(alice, "update_status", %{"life" => 35}), :ok
    assert_broadcast "log_entry", %{id: 3, text: "Alice: 40 → 35 life", count: 2}
    assert [%{id: 4}, %{id: 3} | _] = WebcamTables.log(room)

    # Counter merge metadata survives a saved session.
    assert_reply push(alice, "update_status", %{"poison" => 2}), :ok
    assert_broadcast "log_entry", %{id: 5, counter: %{from: 0, to: 2}}
    assert Session.load(room).log == WebcamTables.log(room)
  end

  test "a seat that stays away past the grace period is logged as leaving", %{room_id: room} do
    bob = join_player(room, @peer_b, "Bob")
    bob_id = bob.assigns.participant.player_id
    disconnect(bob)
    %{departing: %{^bob_id => {"Bob", token}}} = :sys.get_state(room_pid(room))

    # A stale timer from an earlier disconnect is ignored.
    send(room_pid(room), {:departed, bob_id, make_ref()})
    sync_room(room)
    refute_broadcast "log_entry", %{text: "Bob left the table"}

    send(room_pid(room), {:departed, bob_id, token})
    assert_broadcast "log_entry", %{text: "Bob left the table"}
    assert %{text: "Bob left the table"} = hd(WebcamTables.log(room))
  end

  test "an empty room is closed and its session deleted only once idle", %{
    socket: socket,
    room_id: room,
    player: player
  } do
    assert_reply push(socket, "update_status", %{"life" => 3}), :ok

    # Connected rooms are never closed, however long they sit idle.
    assert WebcamTables.close_idle_rooms(0) == []
    assert room in Enum.map(WebcamTables.rooms(), & &1.id)

    disconnect(socket)
    pid = room_pid(room)
    assert WebcamTables.close_idle_rooms(:timer.minutes(30)) == []
    assert room_pid(room) == pid

    # Hold the registry's cleanup so the closed room's entry is still listed,
    # as it may be for a moment after any room stops.
    with_registry_cleanup_paused(fn ->
      ref = Process.monitor(pid)
      assert WebcamTables.close_idle_rooms(0) == [room]
      assert_receive {:DOWN, ^ref, :process, _, :normal}
      assert [{^pid, _opened_at}] = Registry.lookup(TheGathering.WebcamTables.Registry, room)
      refute room in Enum.map(WebcamTables.rooms(), & &1.id)
    end)

    assert Repo.get(Session, room) == nil
    assert rejoin(room, player, @fresh).assigns.participant.life == 40
  end

  test "a seated player cannot be merged away until the table closes", %{
    socket: socket,
    room_id: room,
    player: player
  } do
    user = Repo.preload(player, :user).user
    {:ok, imported} = Games.create_player(%{name: "Alice (imported)"})

    message =
      "Alice has a seat at an open webcam table; record that game and try again " <>
        "once the table closes (30 minutes after everyone leaves)"

    # Linking the account to another player would merge (delete) the seated one,
    # leaving the table's seat and deck ids pointing at rows that no longer exist.
    assert {:error, changeset} = Games.link_player_to_user(imported, user)
    assert errors_on(changeset).merge == [message]
    assert {:error, changeset} = Games.merge_players(player, imported)
    assert errors_on(changeset).merge == [message]
    assert Games.get_player(player.id)

    # Merging into the seated player keeps its id, so the table stays valid.
    {:ok, guest} = Games.create_player(%{name: "Guest"})
    assert {:ok, %{id: id}} = Games.merge_players(guest, player)
    assert id == player.id

    # A departed seat still holds its place until the idle room closes.
    disconnect(socket)
    assert {:error, _changeset} = Games.link_player_to_user(imported, user)

    ref = Process.monitor(room_pid(room))
    assert WebcamTables.close_idle_rooms(0) == [room]
    assert_receive {:DOWN, ^ref, :process, _, :normal}
    assert {:ok, %{id: linked_id}} = Games.link_player_to_user(imported, user)
    assert linked_id == imported.id
    assert Games.get_player(player.id) == nil
  end

  test "expired disconnected sessions are pruned instead of resurrected", %{
    socket: socket,
    room_id: room,
    player: player
  } do
    alias TheGathering.WebcamTables.Session
    assert_reply push(socket, "update_status", %{"life" => 3}), :ok
    disconnect(socket)
    # A restart forgets running rooms, so the next join loads the saved session.
    :ok =
      DynamicSupervisor.terminate_child(TheGathering.WebcamTables.RoomSupervisor, room_pid(room))

    Repo.update_all(from(s in Session, where: s.id == ^room),
      set: [expires_at: DateTime.add(DateTime.utc_now(), -1, :second)]
    )

    assert Session.load(room) == nil
    assert {1, nil} = Session.prune()
    assert rejoin(room, player, @fresh).assigns.participant.life == 40
  end
end
