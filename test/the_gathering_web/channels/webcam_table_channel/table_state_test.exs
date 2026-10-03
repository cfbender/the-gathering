defmodule TheGatheringWeb.WebcamTableChannel.TableStateTest do
  # Shared table state: counters, monarch, seat order, reveal, timers, spectators,
  # rolls, elimination, turns and identified cards.
  use TheGatheringWeb.WebcamTableChannelCase

  alias TheGathering.{Accounts, WebcamTables}
  alias TheGathering.WebcamTables.Timer
  alias TheGatheringWeb.{Presence, UserSocket, WebcamTableChannel}

  @peer_a "00000000-0000-4000-8000-00000000000a"
  @peer_b "00000000-0000-4000-8000-00000000000b"
  @peer_c "00000000-0000-4000-8000-00000000000c"
  @peer_a_new "00000000-0000-4000-8000-0000000000a2"
  @elsewhere "00000000-0000-4000-8000-00000000e0e0"

  test "publishes separate commander counters and rejects invalid updates atomically", %{
    socket: socket,
    room_id: room_id
  } do
    assert_push "presence_state", %{@peer_a => %{metas: [initial]}}
    assert %{poison: 0, rad: 0, commander_casts: %{}, commander_damage: %{}} = initial
    damage = %{"2" => %{"Tymna" => 21, "Thrasios" => 4}, "3" => %{"Tymna" => 8}}
    casts = %{"Tymna" => 3, "Thrasios" => 1}

    assert_reply push(socket, "update_status", %{
                   "poison" => 10,
                   "rad" => 3,
                   "commander_damage" => damage,
                   "commander_casts" => casts
                 }),
                 :ok

    assert_reply push(socket, "update_status", %{"life" => 37}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{poison: 10, rad: 3, life: 37} = meta
    assert meta.commander_damage == damage
    assert meta.commander_casts == casts

    for payload <- [
          %{"poison" => -1},
          %{"rad" => 1.5},
          %{"poison" => "2"},
          %{"rad" => 1000},
          %{"commander_casts" => %{"Tymna" => -1}},
          %{"commander_casts" => %{"" => 2}},
          %{"commander_casts" => []},
          %{"commander_damage" => %{"2" => %{"Tymna" => 1.5}}},
          %{"commander_damage" => %{"peer-ghost" => %{"Tymna" => 1}}},
          %{"commander_damage" => %{"2" => 3}},
          %{"monarch" => true},
          %{"peer_id" => @peer_b}
        ] do
      assert_reply push(socket, "update_status", Map.put(payload, "life", 1)), :error, %{
        reason: "invalid status"
      }
    end

    %{metas: [unchanged]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert unchanged.life == 37
    assert unchanged.commander_damage == damage

    assert_reply push(socket, "update_status", %{
                   "poison" => 0,
                   "rad" => 999,
                   "commander_casts" => %{}
                 }),
                 :ok
  end

  test "publishes shared custom counters and combat buffs, rejecting malformed ones", %{
    socket: socket,
    room_id: room_id
  } do
    assert_push "presence_state", %{@peer_a => %{metas: [initial]}}
    assert %{custom_counters: [], combat_effects: []} = initial

    counters = [%{"id" => "c1", "label" => "Lands", "value" => 7}]

    effects = [
      %{
        "id" => "e1",
        "name" => "Intangible Virtue",
        "power" => 1,
        "toughness" => 1,
        "conditions" => ["Token"],
        "keywords" => ["vigilance"]
      }
    ]

    assert_reply push(socket, "update_status", %{
                   "custom_counters" => counters,
                   "combat_effects" => effects
                 }),
                 :ok

    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert meta.custom_counters == counters
    assert meta.combat_effects == effects
    assert [%{custom_counters: ^counters}] = WebcamTables.snapshot(room_id).seats
    assert Enum.any?(WebcamTables.log(room_id), &(&1.text == "Alice Lands: 0 → 7"))

    effect = hd(effects)

    for payload <- [
          %{"custom_counters" => %{}},
          %{"custom_counters" => [%{"id" => "c1", "label" => "Lands", "value" => 101}]},
          %{"custom_counters" => [%{"id" => "c1", "label" => "", "value" => 1}]},
          %{"custom_counters" => [%{"id" => "c1", "label" => "Lands", "value" => 1, "x" => 1}]},
          %{"custom_counters" => [%{"label" => "Lands", "value" => 1}]},
          %{"combat_effects" => [Map.put(effect, "power", 100)]},
          %{"combat_effects" => [Map.put(effect, "toughness", "1")]},
          %{"combat_effects" => [Map.put(effect, "conditions", "Token")]},
          %{"combat_effects" => [Map.put(effect, "keywords", [""])]},
          %{"combat_effects" => [Map.delete(effect, "keywords")]},
          %{"combat_effects" => [Map.put(effect, "shared", true)]}
        ] do
      assert_reply push(socket, "update_status", payload), :error, %{reason: "invalid status"}
    end

    %{metas: [unchanged]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert unchanged.custom_counters == counters
    assert unchanged.combat_effects == effects

    assert_reply push(socket, "update_status", %{"custom_counters" => [], "combat_effects" => []}),
                 :ok
  end

  test "monarch is one shared holder, synchronized to late joiners and retained on departure", %{
    socket: socket,
    room_id: room_id
  } do
    Process.flag(:trap_exit, true)
    assert_push "monarch_state", %{holder: nil}
    assert_reply push(socket, "take_monarch", %{"peer_id" => "other"}), :error
    assert_reply push(socket, "take_monarch", %{}), :ok
    assert_broadcast "monarch", %{holder: %{peer_id: @peer_a, player_name: "Alice"}}

    bob = join_player(room_id, @peer_b, "Bob")
    assert_push "monarch_state", %{holder: %{peer_id: @peer_a}}
    assert_reply push(bob, "take_monarch", %{}), :ok
    # Both channel transports deliver to this test process.
    assert_broadcast "monarch", %{holder: %{peer_id: @peer_b, player_name: "Bob"}}
    assert_broadcast "monarch", %{holder: %{peer_id: @peer_b, player_name: "Bob"}}
    assert_reply push(bob, "take_monarch", %{}), :ok
    refute_broadcast "monarch", _payload

    # The previous holder leaving must not clear Bob's crown.
    assert_reply leave(socket), :ok
    refute_broadcast "monarch", %{holder: nil}
    join_player(room_id, @peer_c, "Cara")
    assert_push "monarch_state", %{holder: %{peer_id: @peer_b}}
    assert_reply leave(bob), :ok
    refute_broadcast "monarch", %{holder: nil}
    assert WebcamTables.snapshot(room_id).monarch.holder.peer_id == @peer_b
  end

  test "concurrent monarch claims converge on the last serialized event", %{
    socket: alice,
    room_id: room_id
  } do
    assert_push "monarch_state", %{holder: nil}
    bob = join_player(room_id, @peer_b, "Bob")
    assert_push "monarch_state", %{holder: nil}
    alice_ref = push(alice, "take_monarch", %{})
    bob_ref = push(bob, "take_monarch", %{})
    assert_reply alice_ref, :ok
    assert_reply bob_ref, :ok
    assert_broadcast "monarch", %{holder: first, revision: first_revision}
    assert_broadcast "monarch", %{holder: second}
    assert_broadcast "monarch", %{holder: third}
    assert_broadcast "monarch", %{holder: last, revision: last_revision}
    assert first_revision < last_revision

    assert Enum.frequencies_by([first, second, third, last], & &1.peer_id) == %{
             @peer_a => 2,
             @peer_b => 2
           }

    join_player(room_id, @peer_c, "Cara")
    assert_push "monarch_state", %{holder: ^last, revision: snapshot_revision}
    assert snapshot_revision == last_revision
  end

  test "broadcasts a seat order that names every present peer", %{socket: socket} do
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a]}), :ok
    assert_broadcast "seat_order", %{peer_ids: [@peer_a]}

    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a, "peer-ghost"]}),
                 :error,
                 %{reason: "seat order must list every seated player"}

    assert_reply push(socket, "seat_order", %{"peer_ids" => @peer_a}), :error, %{
      reason: "invalid seat order"
    }
  end

  test "reveal validates targets, preserves status, and ends when the target leaves", %{
    socket: socket,
    room_id: room_id
  } do
    other = join_seat(room_id, @peer_b)
    assert_reply push(socket, "reveal", %{"target" => @peer_b}), :ok
    assert_reply push(socket, "update_status", %{"life" => 31}), :ok

    assert %{metas: [%{reveal_to: @peer_b, life: 31}]} =
             Presence.get_by_key(socket.topic, @peer_a)

    for target <- [@peer_a, "absent", ""] do
      assert_reply push(socket, "reveal", %{"target" => target}), :error
    end

    for payload <- [%{"target" => 123}, %{}, %{"target" => nil, "peer_id" => @peer_b}] do
      assert_reply push(socket, "reveal", payload), :error
    end

    assert_reply push(socket, "update_status", %{"reveal_to" => @peer_b}), :error
    assert_reply push(socket, "reveal", %{"target" => nil}), :ok
    assert %{metas: [%{reveal_to: nil}]} = Presence.get_by_key(socket.topic, @peer_a)
    assert_reply push(socket, "reveal", %{"target" => @peer_b}), :ok

    Process.unlink(other.channel_pid)
    leave(other)
    # Match the reveal-clear diff rather than waiting an arbitrary amount of time.
    assert_push "presence_diff", %{joins: %{@peer_a => %{metas: [%{reveal_to: nil}]}}}
    # The earlier manual clear can also be queued, so synchronize on the target's leave.
    assert_push "presence_diff", %{leaves: %{@peer_b => _}}, 1_000
    _ = :sys.get_state(socket.channel_pid)
    assert %{metas: [%{reveal_to: nil}]} = Presence.get_by_key(socket.topic, @peer_a)
  end

  test "server timestamps start, pause and resume; reordering preserves timer", %{socket: socket} do
    assert_push "table_state", %{timer: %{started_at: nil}, peer_ids: []}
    before_start = System.system_time(:millisecond)
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a]}), :ok
    # Starting holds the clock at zero for mulligans until the first player begins play.
    assert_broadcast "timer_state", %{started_at: started, paused_at: started, paused_ms: 0}
    assert started >= before_start
    assert started <= System.system_time(:millisecond)
    assert_reply push(socket, "begin_play", %{}), :ok, running
    assert %{started_at: ^started, paused_at: nil} = running

    assert_reply push(socket, "timer", %{"action" => "pause"}), :ok, paused
    assert paused.started_at == started
    assert is_integer(paused.paused_at)
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a]}), :ok
    assert_reply push(socket, "timer_sync", %{}), :ok, still_paused
    assert still_paused.paused_at == paused.paused_at
    assert still_paused.started_at == started

    assert_reply push(socket, "timer", %{"action" => "resume"}), :ok, resumed
    assert resumed.paused_at == nil
    assert resumed.started_at == started
    assert resumed.paused_ms >= 0
    assert_reply push(socket, "timer", %{"action" => "resume"}), :ok, repeated
    assert repeated.paused_ms == resumed.paused_ms
  end

  test "late arrivals receive state as spectators and cannot mutate it", %{
    socket: socket,
    room_id: room_id
  } do
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a]}), :ok
    assert_reply push(socket, "timer", %{"action" => "pause"}), :ok, timer
    other = join_player(room_id, @peer_b, "Bob")
    paused_at = timer.paused_at

    assert_push "table_state", %{
      timer: %{paused_at: ^paused_at} = joined_timer,
      peer_ids: [@peer_a]
    }

    assert Map.drop(joined_timer, [:server_now]) == Map.drop(timer, [:server_now])
    assert other.assigns.participant.spectator
    assert Enum.map(WebcamTables.snapshot(room_id).seats, & &1.peer_id) == [@peer_a]

    for {event, payload} <- [
          {"timer", %{"action" => "resume"}},
          {"update_status", %{"life" => 7}},
          {"start_game", %{}},
          {"pass_turn", %{"revision" => 1}},
          {"unpass_turn", %{"revision" => 1}},
          {"take_monarch", %{}},
          {"set_eliminated", %{"peer_id" => @peer_a, "eliminated" => true}},
          {"cards", %{"type" => "cards_cleared", "ownerPeerId" => @peer_a}}
        ] do
      assert_reply push(other, event, payload), :error, %{
        reason: "spectators cannot change the game"
      }
    end

    assert_reply push(other, "timer_sync", %{}), :ok, %{paused_at: ^paused_at}
    assert_reply push(socket, "timer", %{"action" => "resume"}), :ok, resumed
    assert resumed.started_at == timer.started_at
    assert resumed.paused_at == nil
    assert_broadcast "timer_state", %{paused_at: nil, started_at: started}
    assert started == timer.started_at

    join_player(Ecto.UUID.generate(), @peer_c, "Cara")
    assert_push "table_state", %{timer: %{started_at: nil, paused_ms: 0}, peer_ids: []}
  end

  test "only the first player or the owner can end the mulligan window", %{
    socket: socket,
    room_id: room_id,
    player: player
  } do
    other = join_player(room_id, @peer_b, "Bob")
    assert_reply push(socket, "begin_play", %{}), :ok, %{started_at: nil}
    assert_reply push(socket, "turn_settings", %{"auto_randomize" => false}), :ok
    assert_reply push(socket, "arrange_seats", %{"peer_ids" => [@peer_b, @peer_a]}), :ok
    assert_reply push(socket, "start_game", %{}), :ok
    bob = other.assigns.participant.player_id
    assert %{turns: %{active_player_id: ^bob}} = WebcamTables.snapshot(room_id)

    # Bob goes first, so Alice acting as a plain seat (not as owner) is refused.
    assert {:error, %{reason: "only the first player can start the game"}} =
             WebcamTables.begin_play(room_id, player.id)

    assert_reply push(socket, "begin_play", %{"at" => 1}), :error, %{reason: "invalid start"}
    assert_reply push(other, "begin_play", %{}), :ok, %{paused_at: nil} = running
    assert_broadcast "timer_state", %{paused_at: nil}

    # Once running, repeated starts change nothing.
    assert_reply push(socket, "begin_play", %{}), :ok, again
    assert Map.drop(again, [:server_now]) == Map.drop(running, [:server_now])
  end

  test "rejects forged timestamps, unknown timer actions and invalid sync payloads", %{
    socket: socket
  } do
    for payload <- [
          %{"action" => "start"},
          %{"action" => "reset"},
          %{},
          %{"action" => "pause", "started_at" => 1}
        ] do
      assert_reply push(socket, "timer", payload), :error, %{reason: "invalid timer action"}
    end

    assert_reply push(socket, "timer_sync", %{"server_now" => 1}), :error
    assert_reply push(socket, "timer_sync", %{}), :ok, %{started_at: nil}
  end

  test "server generates attributed dice and coin rolls, rejecting forged or invalid payloads", %{
    socket: socket
  } do
    for sides <- [2, 6, 20, 1000] do
      assert_reply push(socket, "roll", %{"kind" => "dice", "sides" => sides}), :ok

      assert_broadcast "roll", %{
        kind: "dice",
        sides: ^sides,
        result: result,
        actor: @peer_a,
        player_name: "Alice",
        at: at,
        id: id
      }

      assert result in 1..sides
      assert is_integer(at)
      assert {:ok, _} = Ecto.UUID.cast(id)
    end

    assert_reply push(socket, "roll", %{"kind" => "coin"}), :ok
    assert_broadcast "roll", %{kind: "coin", result: result, player_name: "Alice"}
    assert result in ["Heads", "Tails"]

    for payload <- [
          %{"kind" => "dice", "sides" => 1},
          %{"kind" => "dice", "sides" => 1001},
          %{"kind" => "dice", "sides" => 6.5},
          %{"kind" => "dice", "sides" => "20"},
          %{"kind" => "dice", "sides" => 20, "result" => 20},
          %{"kind" => "coin", "player_name" => "Bob"},
          %{"kind" => "coin", "result" => "Heads"},
          %{"kind" => "other"},
          %{}
        ] do
      assert_reply push(socket, "roll", payload), :error
    end

    refute_broadcast "roll", _
  end

  test "validates elimination in own status without changing life", %{
    socket: socket,
    room_id: room_id
  } do
    assert_reply push(socket, "update_status", %{"eliminated" => true}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{eliminated: true, life: 40} = meta
    assert_reply push(socket, "update_status", %{"eliminated" => "true"}), :error
    assert_reply push(socket, "update_status", %{"eliminated" => false}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert meta.eliminated == false
  end

  test "reaching zero life eliminates the seat; regaining life does not restore it", %{
    socket: socket,
    room_id: room_id
  } do
    assert_reply push(socket, "update_status", %{"life" => 1}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{eliminated: false, life: 1} = meta
    refute_broadcast "eliminated_seats", %{}

    assert_reply push(socket, "update_status", %{"life" => 0}), :ok
    assert_broadcast "eliminated_seats", %{participants: [%{peer_id: @peer_a, eliminated: true}]}
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{eliminated: true, life: 0} = meta

    assert_reply push(socket, "update_status", %{"life" => 5}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{eliminated: true, life: 5} = meta

    # An explicit restore in the same update wins over the zero-life rule.
    assert_reply push(socket, "update_status", %{"life" => -3, "eliminated" => false}), :ok
    assert_broadcast "eliminated_seats", %{participants: []}
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{eliminated: false, life: -3} = meta
  end

  test "only the owner or the seat itself can eliminate and restore a player", %{
    socket: socket,
    room_id: room_id
  } do
    other = join_player(room_id, @peer_b, "Bob")

    assert_reply push(other, "set_eliminated", %{"peer_id" => @peer_a, "eliminated" => true}),
                 :error

    assert_reply push(socket, "set_eliminated", %{"peer_id" => @peer_a, "eliminated" => true}),
                 :ok

    assert_broadcast "eliminated_seats", %{participants: [%{peer_id: @peer_a, eliminated: true}]}
    assert_reply push(socket, "update_status", %{"life" => 7}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{eliminated: true, life: 7} = meta

    assert_reply push(socket, "set_eliminated", %{"peer_id" => @peer_a, "eliminated" => false}),
                 :ok

    assert_broadcast "eliminated_seats", %{participants: []}
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert meta.eliminated == false

    for payload <- [
          %{},
          %{"peer_id" => @peer_a, "eliminated" => 1},
          %{"peer_id" => @peer_a, "eliminated" => true, "life" => 0}
        ] do
      assert_reply push(other, "set_eliminated", payload), :error, %{
        reason: "invalid elimination"
      }
    end

    assert_reply push(other, "set_eliminated", %{"peer_id" => "ghost", "eliminated" => true}),
                 :error

    foreign = join_player(Ecto.UUID.generate(), @elsewhere, "Cara")

    assert_reply push(foreign, "set_eliminated", %{"peer_id" => @peer_a, "eliminated" => true}),
                 :error
  end

  test "departed eliminated seats survive for results and late joins; rejoining replaces their peer id",
       %{socket: socket, room_id: room_id, player: player} do
    other = join_player(room_id, @peer_b, "Bob")
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a, @peer_b]}), :ok
    assert_reply push(socket, "update_status", %{"eliminated" => true}), :ok

    %{metas: [%{phx_ref: presence_ref}]} =
      Presence.get_by_key("webcam_table:#{room_id}", @peer_a)

    Process.unlink(socket.channel_pid)
    ref = Process.monitor(socket.channel_pid)
    assert_reply leave(socket), :ok
    assert_receive {:DOWN, ^ref, :process, _, _}

    assert_broadcast "presence_diff", %{
      leaves: %{@peer_a => %{metas: [%{phx_ref: ^presence_ref}]}}
    }

    assert_reply push(other, "seat_order", %{"peer_ids" => [@peer_b]}), :error

    assert %{peer_ids: [@peer_a, @peer_b], eliminated_seats: [%{player_id: id}]} =
             WebcamTables.snapshot(room_id)

    assert id == player.id

    join_player(room_id, @peer_c, "Cara")
    assert_push "table_state", %{eliminated_seats: [%{player_id: ^id, eliminated: true}]}

    rejoined =
      UserSocket
      |> socket(@peer_a_new, %{user: Accounts.get_user(player.user_id)})
      |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room_id}", %{
        "peer_id" => @peer_a_new,
        "player_id" => player.id
      })

    assert_push "table_state", %{
      peer_ids: [@peer_a_new, @peer_b],
      eliminated_seats: [%{peer_id: @peer_a_new}]
    }

    assert_reply push(rejoined, "update_status", %{"eliminated" => false}), :ok
    assert WebcamTables.snapshot(room_id).eliminated_seats == []
  end

  test "shared turns start in order, pass once per revision and survive late joins", %{
    socket: socket,
    room_id: room_id,
    player: player
  } do
    other = join_player(room_id, @peer_b, "Bob")
    bob = other.assigns.participant.player_id
    alice = player.id
    assert_reply push(socket, "pass_turn", %{"revision" => 0}), :error
    assert_reply push(other, "turn_settings", %{"auto_randomize" => false}), :error
    assert_reply push(socket, "turn_settings", %{"auto_randomize" => false}), :ok
    assert_broadcast "table_state", %{auto_randomize: false}
    assert_reply push(socket, "start_game", %{}), :ok
    assert_broadcast "seat_order", %{peer_ids: [@peer_a, @peer_b], shuffled: false}

    assert_broadcast "table_state", %{
      turns: %{active_player_id: ^alice, counts: %{^alice => 1}, revision: 1}
    }

    first = WebcamTables.snapshot(room_id)
    assert first.timer.paused_at == first.timer.started_at
    assert_reply push(other, "pass_turn", %{"revision" => 1}), :ok
    assert_reply push(socket, "pass_turn", %{"revision" => 1}), :error
    # Passing the first turn before pressing Start still begins the clock.
    assert_broadcast "timer_state", %{paused_at: nil}

    assert_broadcast "table_state", %{
      turns: %{active_player_id: ^bob, counts: %{^alice => 1, ^bob => 1}, revision: 2}
    }

    # Un-pass hands the turn back once per revision, and passing again restores it.
    assert_reply push(socket, "unpass_turn", %{"revision" => 1}), :error
    assert_reply push(socket, "unpass_turn", %{}), :error
    assert_reply push(socket, "unpass_turn", %{"revision" => 2}), :ok

    assert_broadcast "table_state", %{
      turns: %{active_player_id: ^alice, counts: %{^alice => 1, ^bob => 0}, revision: 3}
    }

    assert_reply push(other, "unpass_turn", %{"revision" => 3}), :error
    assert_reply push(other, "pass_turn", %{"revision" => 3}), :ok

    assert_broadcast "table_state", %{
      turns: %{active_player_id: ^bob, counts: %{^alice => 1, ^bob => 1}, revision: 4}
    }

    assert_reply push(socket, "adjust_turn", %{"player_id" => alice, "delta" => 1}), :ok
    assert WebcamTables.snapshot(room_id).turns.counts[alice] == 2
    assert_reply push(socket, "timer", %{"action" => "pause"}), :ok, paused
    assert_reply push(socket, "pass_turn", %{"revision" => 4}), :ok
    current = WebcamTables.snapshot(room_id)
    assert current.turns.counts == %{alice => 3, bob => 1}
    assert current.turns.active_player_id == alice
    assert current.turns.started_elapsed_ms == Timer.elapsed(paused, paused.server_now)
    assert current.timer.started_at == first.timer.started_at
    join_player(room_id, @peer_c, "Cara")
    expected = current.turns
    assert_push "table_state", %{turns: ^expected, auto_randomize: false}
    # A reshuffle leaves the active player, counts and pause untouched.
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_c, @peer_b, @peer_a]}),
                 :error

    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_b, @peer_a]}), :ok
    assert WebcamTables.snapshot(room_id).turns == expected
    assert WebcamTables.snapshot(room_id).timer.paused_at == paused.paused_at
  end

  test "eliminating skips turns but disconnecting does not advance the game", %{
    socket: socket,
    room_id: room_id,
    player: player
  } do
    other = join_player(room_id, @peer_b, "Bob")
    bob = other.assigns.participant.player_id
    alice = player.id
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a, @peer_b]}), :ok
    assert_reply push(socket, "update_status", %{"eliminated" => true}), :ok
    assert_broadcast "table_state", %{turns: %{active_player_id: ^bob, revision: 2}}
    assert_reply push(other, "pass_turn", %{"revision" => 2}), :ok
    assert WebcamTables.snapshot(room_id).turns.counts == %{alice => 1, bob => 2}
    assert_reply push(socket, "update_status", %{"eliminated" => false}), :ok
    Process.unlink(other.channel_pid)
    ref = Process.monitor(other.channel_pid)
    assert_reply leave(other), :ok
    assert_receive {:DOWN, ^ref, :process, _, _}
    sync_room(room_id)

    assert %{active_player_id: ^bob, counts: %{^alice => 1, ^bob => 2}, revision: 3} =
             WebcamTables.snapshot(room_id).turns
  end

  test "validates turn requests and rejects forged counts, times and unknown players", %{
    socket: socket
  } do
    for {event, payload} <- [
          {"start_game", %{"started_at" => 1}},
          {"turn_settings", %{"auto_randomize" => "false"}},
          {"turn_settings", %{"auto_randomize" => true, "order" => []}},
          {"pass_turn", %{}},
          {"pass_turn", %{"revision" => -1}},
          {"pass_turn", %{"revision" => 1.5}},
          {"pass_turn", %{"revision" => 0, "elapsed_ms" => 0}},
          {"adjust_turn", %{"player_id" => -1, "delta" => 1}},
          {"adjust_turn", %{"player_id" => 1, "delta" => 2}},
          {"adjust_turn", %{"player_id" => 1, "delta" => -1, "count" => 999}},
          {"adjust_turn", %{"player_id" => "1", "delta" => 1}}
        ] do
      assert_reply push(socket, event, payload), :error
    end
  end

  describe "identified cards" do
    defp card_entry(owner, overrides \\ %{}) do
      Map.merge(
        %{
          "id" => Ecto.UUID.generate(),
          "ownerPeerId" => owner,
          "at" => 123,
          "card" => %{"id" => "art-1", "name" => "Forest", "set" => "lea"}
        },
        overrides
      )
    end

    test "attribution is stamped from the sender's seat, never the payload", %{
      socket: alice,
      room_id: room
    } do
      bob = join_player(room, @peer_b, "Bob")
      spoofed = card_entry(@peer_a, %{"byPlayerName" => "Alice"})

      assert_reply push(bob, "cards", %{"type" => "card_identified", "entry" => spoofed}), :ok

      assert_broadcast "identified_cards", %{
        entries: [%{"byPlayerName" => "Bob"}],
        type: "card_identified",
        by: %{peer_id: @peer_b, player_name: "Bob"}
      }

      assert [%{"byPlayerName" => "Bob"}] = WebcamTables.snapshot(room).cards

      # The name is optional on the wire.
      unnamed =
        card_entry(@peer_b, %{"card" => %{"id" => "a", "name" => "Island", "set" => "x"}})

      assert_reply push(alice, "cards", %{"type" => "card_identified", "entry" => unnamed}), :ok

      assert [%{"byPlayerName" => "Bob"}, %{"byPlayerName" => "Alice"}] =
               WebcamTables.snapshot(room).cards
    end

    test "any seat may remove an entry and the broadcast names the remover", %{
      socket: alice,
      room_id: room
    } do
      bob = join_player(room, @peer_b, "Bob")
      entry = card_entry(@peer_a)
      assert_reply push(alice, "cards", %{"type" => "card_identified", "entry" => entry}), :ok
      assert_reply push(bob, "cards", %{"type" => "card_removed", "id" => entry["id"]}), :ok

      assert_broadcast "identified_cards", %{
        entries: [],
        type: "card_removed",
        by: %{peer_id: @peer_b, player_name: "Bob"}
      }

      assert WebcamTables.snapshot(room).cards == []
    end

    test "only the board owner can clear its cards", %{socket: alice, room_id: room} do
      bob = join_player(room, @peer_b, "Bob")
      entry = card_entry(@peer_a)
      assert_reply push(alice, "cards", %{"type" => "card_identified", "entry" => entry}), :ok

      assert_reply push(bob, "cards", %{"type" => "cards_cleared", "ownerPeerId" => @peer_a}),
                   :error,
                   %{reason: "only the board owner can clear its cards"}

      assert length(WebcamTables.snapshot(room).cards) == 1

      assert_reply push(alice, "cards", %{"type" => "cards_cleared", "ownerPeerId" => @peer_a}),
                   :ok

      assert WebcamTables.snapshot(room).cards == []
    end

    test "spectators cannot identify, remove or clear cards", %{socket: alice, room_id: room} do
      entry = card_entry(@peer_a)
      # Starting the game makes later arrivals spectators (and clears lobby cards).
      assert_reply push(alice, "seat_order", %{"peer_ids" => [@peer_a]}), :ok
      assert_reply push(alice, "cards", %{"type" => "card_identified", "entry" => entry}), :ok
      spectator = join_player(room, @peer_b, "Bob")
      assert spectator.assigns.participant.spectator

      for payload <- [
            %{"type" => "card_identified", "entry" => card_entry(@peer_a)},
            %{"type" => "card_removed", "id" => entry["id"]},
            %{"type" => "cards_cleared", "ownerPeerId" => @peer_b}
          ] do
        assert_reply push(spectator, "cards", payload), :error, %{
          reason: "spectators cannot change the game"
        }
      end

      assert [%{"id" => id}] = WebcamTables.snapshot(room).cards
      assert id == entry["id"]
    end
  end
end
