defmodule TheGatheringWeb.WebcamTableChannelTest do
  use TheGathering.DataCase, async: false
  import Phoenix.ChannelTest

  alias TheGathering.{Accounts, AccountsFixtures, Games}

  alias TheGatheringWeb.{
    Presence,
    UserSocket,
    WebcamTableChannel,
    WebcamTableRooms,
    WebcamTableState
  }

  @endpoint TheGatheringWeb.Endpoint

  test "socket connection uses the tracked cookie session" do
    user = AccountsFixtures.user_fixture()
    session_token = Accounts.generate_user_session_token(user)
    token = Phoenix.Token.sign(TheGatheringWeb.Endpoint, "webcam table socket", session_token)
    socket = socket(UserSocket, nil, %{})

    assert {:ok, connected} = UserSocket.connect(%{"token" => token}, socket, %{})

    assert connected.assigns.user.id == user.id
    assert UserSocket.id(connected) == "users_sessions:#{Base.url_encode64(session_token)}"
    assert :error = UserSocket.connect(%{"token" => "invalid"}, socket, %{})
  end

  setup do
    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: "Alice"}, user.id)

    {:ok, deck} =
      Games.create_deck(%{player_id: player.id, name: "Birds", commander_name: "Kangee"})

    room_id = Ecto.UUID.generate()

    socket =
      UserSocket
      |> socket("peer-a", %{user: user})
      |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room_id}", %{
        "peer_id" => "peer-a",
        "player_id" => player.id
      })

    %{socket: socket, player: player, deck: deck, room_id: room_id}
  end

  test "joins with a real player and relays targeted signaling", %{socket: socket} do
    assert_push "presence_state", %{"peer-a" => %{metas: [meta]}}
    assert meta.player_name == "Alice"

    push(socket, "signal", %{"target" => "peer-b", "signal" => %{"candidate" => "ice"}})

    assert_broadcast "signal", %{
      target: "peer-b",
      from: "peer-a",
      signal: %{"candidate" => "ice"}
    }
  end

  test "updates presence only with a deck owned by the seated player", %{
    socket: socket,
    deck: deck,
    room_id: room_id
  } do
    assert_reply push(socket, "choose_deck", %{"deck_id" => deck.id}), :ok
    deck_id = deck.id
    assert_broadcast "deck_selected", %{deck_id: ^deck_id}
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert meta.deck_id == deck.id

    # Reselecting after an art/partner edit must refresh peer caches even with the same ID.
    assert_reply push(socket, "choose_deck", %{"deck_id" => deck.id}), :ok
    assert_broadcast "deck_selected", %{deck_id: ^deck_id}

    {:ok, other} = Games.create_player(%{name: "Bob"})

    {:ok, other_deck} =
      Games.create_deck(%{player_id: other.id, name: "Dragons", commander_name: "Miirym"})

    assert_reply push(socket, "choose_deck", %{"deck_id" => other_deck.id}), :error, %{
      reason: "deck does not belong to player"
    }

    refute_broadcast "deck_selected", _
  end

  test "publishes life and camera status through presence", %{socket: socket, room_id: room_id} do
    assert_push "presence_state", %{"peer-a" => %{metas: [meta]}}
    assert %{life: 40, camera_off: false, joined_at: joined_at} = meta
    assert is_integer(joined_at)

    assert_reply push(socket, "update_status", %{"life" => 37, "camera_off" => true}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert %{life: 37, camera_off: true} = meta
    refute Map.has_key?(meta, :muted)

    assert_reply push(socket, "update_status", %{"life" => 1_000}), :error, %{
      reason: "invalid status"
    }

    assert_reply push(socket, "update_status", %{"life" => 20, "role" => "admin"}), :error, %{
      reason: "invalid status"
    }

    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert meta.life == 37
  end

  test "publishes separate commander counters and rejects invalid updates atomically", %{
    socket: socket,
    room_id: room_id
  } do
    assert_push "presence_state", %{"peer-a" => %{metas: [initial]}}
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
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
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
          %{"peer_id" => "peer-b"}
        ] do
      assert_reply push(socket, "update_status", Map.put(payload, "life", 1)), :error, %{
        reason: "invalid status"
      }
    end

    %{metas: [unchanged]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert unchanged.life == 37
    assert unchanged.commander_damage == damage

    assert_reply push(socket, "update_status", %{
                   "poison" => 0,
                   "rad" => 999,
                   "commander_casts" => %{}
                 }),
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
    assert_broadcast "monarch", %{holder: %{peer_id: "peer-a", player_name: "Alice"}}

    bob = join_player(room_id, "peer-b", "Bob")
    assert_push "monarch_state", %{holder: %{peer_id: "peer-a"}}
    assert_reply push(bob, "take_monarch", %{}), :ok
    # Both channel transports deliver to this test process.
    assert_broadcast "monarch", %{holder: %{peer_id: "peer-b", player_name: "Bob"}}
    assert_broadcast "monarch", %{holder: %{peer_id: "peer-b", player_name: "Bob"}}
    assert_reply push(bob, "take_monarch", %{}), :ok
    refute_broadcast "monarch", _payload

    # The previous holder leaving must not clear Bob's crown.
    assert_reply leave(socket), :ok
    refute_broadcast "monarch", %{holder: nil}
    join_player(room_id, "peer-c", "Cara")
    assert_push "monarch_state", %{holder: %{peer_id: "peer-b"}}
    assert_reply leave(bob), :ok
    refute_broadcast "monarch", %{holder: nil}
    assert WebcamTableState.snapshot(room_id).monarch.holder.peer_id == "peer-b"
  end

  test "concurrent monarch claims converge on the last serialized event", %{
    socket: alice,
    room_id: room_id
  } do
    assert_push "monarch_state", %{holder: nil}
    bob = join_player(room_id, "peer-b", "Bob")
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
             "peer-a" => 2,
             "peer-b" => 2
           }

    join_player(room_id, "peer-c", "Cara")
    assert_push "monarch_state", %{holder: ^last, revision: snapshot_revision}
    assert snapshot_revision == last_revision
  end

  defp join_player(room_id, peer_id, name) do
    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: name}, user.id)

    UserSocket
    |> socket(peer_id, %{user: user})
    |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room_id}", %{
      "peer_id" => peer_id,
      "player_id" => player.id
    })
  end

  test "broadcasts a seat order that names every present peer", %{socket: socket} do
    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-a"]}), :ok
    assert_broadcast "seat_order", %{peer_ids: ["peer-a"]}

    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-a", "peer-ghost"]}),
                 :error,
                 %{reason: "seat order must list every seated player"}

    assert_reply push(socket, "seat_order", %{"peer_ids" => "peer-a"}), :error, %{
      reason: "invalid seat order"
    }
  end

  test "rejects invalid room IDs", %{player: player} do
    user = AccountsFixtures.user_fixture()
    socket = socket(UserSocket, "peer-b", %{user: user})

    assert {:error, %{reason: "room is full or invalid"}} =
             subscribe_and_join(socket, WebcamTableChannel, "webcam_table:not-a-uuid", %{
               "peer_id" => "peer-b",
               "player_id" => player.id
             })
  end

  test "reveal validates targets, preserves status, and ends when the target leaves", %{
    socket: socket,
    room_id: room_id
  } do
    other = join_seat(room_id, "peer-b")
    assert_reply push(socket, "reveal", %{"target" => "peer-b"}), :ok
    assert_reply push(socket, "update_status", %{"life" => 31}), :ok

    assert %{metas: [%{reveal_to: "peer-b", life: 31}]} =
             Presence.get_by_key(socket.topic, "peer-a")

    for target <- ["peer-a", "absent", ""] do
      assert_reply push(socket, "reveal", %{"target" => target}), :error
    end

    for payload <- [%{"target" => 123}, %{}, %{"target" => nil, "peer_id" => "peer-b"}] do
      assert_reply push(socket, "reveal", payload), :error
    end

    assert_reply push(socket, "update_status", %{"reveal_to" => "peer-b"}), :error
    assert_reply push(socket, "reveal", %{"target" => nil}), :ok
    assert %{metas: [%{reveal_to: nil}]} = Presence.get_by_key(socket.topic, "peer-a")
    assert_reply push(socket, "reveal", %{"target" => "peer-b"}), :ok

    Process.unlink(other.channel_pid)
    leave(other)
    # Match the reveal-clear diff rather than waiting an arbitrary amount of time.
    assert_push "presence_diff", %{joins: %{"peer-a" => %{metas: [%{reveal_to: nil}]}}}
    # The earlier manual clear can also be queued, so synchronize on the target's leave.
    assert_push "presence_diff", %{leaves: %{"peer-b" => _}}, 1_000
    _ = :sys.get_state(socket.channel_pid)
    assert %{metas: [%{reveal_to: nil}]} = Presence.get_by_key(socket.topic, "peer-a")
  end

  test "admits ten seats, marks the lobby full, and refuses the eleventh", %{
    socket: socket,
    room_id: room_id
  } do
    for index <- 2..9, do: join_seat(room_id, "peer-#{index}")
    refute Enum.find(WebcamTableRooms.active_rooms(), &(&1.id == room_id)).full
    join_seat(room_id, "peer-10")
    assert map_size(Presence.list(socket)) == 10
    assert Enum.find(WebcamTableRooms.active_rooms(), &(&1.id == room_id)).full

    {user, player} = linked_player("peer-11")

    assert {:error, %{reason: "room is full"}} =
             UserSocket
             |> socket("peer-11", %{user: user})
             |> subscribe_and_join(WebcamTableChannel, socket.topic, %{
               "peer_id" => "peer-11",
               "player_id" => player.id
             })

    order = ["peer-a" | Enum.map(2..10, &"peer-#{&1}")]
    assert_reply push(socket, "seat_order", %{"peer_ids" => Enum.reverse(order)}), :ok
    assert_broadcast "seat_order", %{peer_ids: peer_ids}
    assert peer_ids == Enum.reverse(order)
  end

  test "rejects empty and duplicate peer IDs", %{socket: seated} do
    {user, player} = linked_player("Bob")

    for peer_id <- ["", "peer-a"] do
      assert {:error, _reason} =
               UserSocket
               |> socket(peer_id, %{user: user})
               |> subscribe_and_join(WebcamTableChannel, seated.topic, %{
                 "peer_id" => peer_id,
                 "player_id" => player.id
               })
    end

    assert map_size(Presence.list(seated)) == 1
  end

  defp linked_player(name) do
    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: name}, user.id)
    {user, player}
  end

  defp join_seat(room_id, peer_id) do
    {user, player} = linked_player(peer_id)

    joined =
      UserSocket
      |> socket(peer_id, %{user: user})
      |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room_id}", %{
        "peer_id" => peer_id,
        "player_id" => player.id
      })

    _ = :sys.get_state(joined.channel_pid)
    joined
  end

  test "server timestamps start, pause and resume; reordering preserves timer", %{socket: socket} do
    assert_push "table_state", %{timer: %{started_at: nil}, peer_ids: []}
    before_start = System.system_time(:millisecond)
    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-a"]}), :ok
    assert_broadcast "timer_state", %{started_at: started, paused_at: nil, paused_ms: 0}
    assert started >= before_start
    assert started <= System.system_time(:millisecond)

    assert_reply push(socket, "timer", %{"action" => "pause"}), :ok, paused
    assert paused.started_at == started
    assert is_integer(paused.paused_at)
    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-a"]}), :ok
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
    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-a"]}), :ok
    assert_reply push(socket, "timer", %{"action" => "pause"}), :ok, timer
    other = join_player(room_id, "peer-b", "Bob")
    paused_at = timer.paused_at

    assert_push "table_state", %{
      timer: %{paused_at: ^paused_at} = joined_timer,
      peer_ids: ["peer-a"]
    }

    assert Map.drop(joined_timer, [:server_now]) == Map.drop(timer, [:server_now])
    assert other.assigns.participant.spectator
    assert Enum.map(WebcamTableState.snapshot(room_id).seats, & &1.peer_id) == ["peer-a"]

    for {event, payload} <- [
          {"timer", %{"action" => "resume"}},
          {"update_status", %{"life" => 7}},
          {"start_game", %{}},
          {"pass_turn", %{"revision" => 1}},
          {"take_monarch", %{}},
          {"set_eliminated", %{"peer_id" => "peer-a", "eliminated" => true}},
          {"cards", %{"type" => "cards_cleared", "ownerPeerId" => "peer-a"}}
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

    join_player(Ecto.UUID.generate(), "peer-c", "Cara")
    assert_push "table_state", %{timer: %{started_at: nil, paused_ms: 0}, peer_ids: []}
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
        actor: "peer-a",
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

  test "timer transitions account for multiple unequal pauses without resetting" do
    timer = WebcamTableState.new_timer()
    assert WebcamTableState.update_timer(timer, "resume", 10) == timer
    timer = WebcamTableState.update_timer(timer, "start", 1000)
    timer = WebcamTableState.update_timer(timer, "pause", 13_000)
    assert WebcamTableState.update_timer(timer, "pause", 20_000) == timer
    timer = WebcamTableState.update_timer(timer, "resume", 22_000)
    assert timer.paused_ms == 9000
    timer = WebcamTableState.update_timer(timer, "pause", 41_000)
    timer = WebcamTableState.update_timer(timer, "resume", 46_000)
    assert timer == %{started_at: 1000, paused_at: nil, paused_ms: 14_000}
    assert WebcamTableState.update_timer(timer, "start", 50_000) == timer
  end

  test "validates elimination in own status without changing life", %{
    socket: socket,
    room_id: room_id
  } do
    assert_reply push(socket, "update_status", %{"eliminated" => true}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert %{eliminated: true, life: 40} = meta
    assert_reply push(socket, "update_status", %{"eliminated" => "true"}), :error
    assert_reply push(socket, "update_status", %{"eliminated" => false}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert meta.eliminated == false
  end

  test "only the owner or the seat itself can eliminate and restore a player", %{
    socket: socket,
    room_id: room_id
  } do
    other = join_player(room_id, "peer-b", "Bob")

    assert_reply push(other, "set_eliminated", %{"peer_id" => "peer-a", "eliminated" => true}),
                 :error

    assert_reply push(socket, "set_eliminated", %{"peer_id" => "peer-a", "eliminated" => true}),
                 :ok

    assert_broadcast "eliminated_seats", %{participants: [%{peer_id: "peer-a", eliminated: true}]}
    assert_reply push(socket, "update_status", %{"life" => 7}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert %{eliminated: true, life: 7} = meta

    assert_reply push(socket, "set_eliminated", %{"peer_id" => "peer-a", "eliminated" => false}),
                 :ok

    assert_broadcast "eliminated_seats", %{participants: []}
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert meta.eliminated == false

    for payload <- [
          %{},
          %{"peer_id" => "peer-a", "eliminated" => 1},
          %{"peer_id" => "peer-a", "eliminated" => true, "life" => 0}
        ] do
      assert_reply push(other, "set_eliminated", payload), :error, %{
        reason: "invalid elimination"
      }
    end

    assert_reply push(other, "set_eliminated", %{"peer_id" => "ghost", "eliminated" => true}),
                 :error

    foreign = join_player(Ecto.UUID.generate(), "elsewhere", "Cara")

    assert_reply push(foreign, "set_eliminated", %{"peer_id" => "peer-a", "eliminated" => true}),
                 :error
  end

  test "departed eliminated seats survive for results and late joins; rejoining replaces their peer id",
       %{socket: socket, room_id: room_id, player: player} do
    other = join_player(room_id, "peer-b", "Bob")
    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-a", "peer-b"]}), :ok
    assert_reply push(socket, "update_status", %{"eliminated" => true}), :ok

    %{metas: [%{phx_ref: presence_ref}]} =
      Presence.get_by_key("webcam_table:#{room_id}", "peer-a")

    Process.unlink(socket.channel_pid)
    ref = Process.monitor(socket.channel_pid)
    assert_reply leave(socket), :ok
    assert_receive {:DOWN, ^ref, :process, _, _}

    assert_broadcast "presence_diff", %{
      leaves: %{"peer-a" => %{metas: [%{phx_ref: ^presence_ref}]}}
    }

    assert_reply push(other, "seat_order", %{"peer_ids" => ["peer-b"]}), :error

    assert %{peer_ids: ["peer-a", "peer-b"], eliminated_seats: [%{player_id: id}]} =
             WebcamTableState.snapshot(room_id)

    assert id == player.id

    join_player(room_id, "peer-c", "Cara")
    assert_push "table_state", %{eliminated_seats: [%{player_id: ^id, eliminated: true}]}

    rejoined =
      UserSocket
      |> socket("peer-a-new", %{user: Accounts.get_user(player.user_id)})
      |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room_id}", %{
        "peer_id" => "peer-a-new",
        "player_id" => player.id
      })

    assert_push "table_state", %{
      peer_ids: ["peer-a-new", "peer-b"],
      eliminated_seats: [%{peer_id: "peer-a-new"}]
    }

    assert_reply push(rejoined, "update_status", %{"eliminated" => false}), :ok
    assert WebcamTableState.snapshot(room_id).eliminated_seats == []
  end

  test "shared turns start in order, pass once per revision and survive late joins", %{
    socket: socket,
    room_id: room_id,
    player: player
  } do
    other = join_player(room_id, "peer-b", "Bob")
    bob = other.assigns.participant.player_id
    alice = player.id
    assert_reply push(socket, "pass_turn", %{"revision" => 0}), :error
    assert_reply push(other, "turn_settings", %{"auto_randomize" => false}), :error
    assert_reply push(socket, "turn_settings", %{"auto_randomize" => false}), :ok
    assert_broadcast "table_state", %{auto_randomize: false}
    assert_reply push(socket, "start_game", %{}), :ok
    assert_broadcast "seat_order", %{peer_ids: ["peer-a", "peer-b"], shuffled: false}

    assert_broadcast "table_state", %{
      turns: %{active_player_id: ^alice, counts: %{^alice => 1}, revision: 1}
    }

    first = WebcamTableState.snapshot(room_id)
    assert_reply push(other, "pass_turn", %{"revision" => 1}), :ok
    assert_reply push(socket, "pass_turn", %{"revision" => 1}), :error

    assert_broadcast "table_state", %{
      turns: %{active_player_id: ^bob, counts: %{^alice => 1, ^bob => 1}, revision: 2}
    }

    assert_reply push(socket, "adjust_turn", %{"player_id" => alice, "delta" => 1}), :ok
    assert WebcamTableState.snapshot(room_id).turns.counts[alice] == 2
    assert_reply push(socket, "timer", %{"action" => "pause"}), :ok, paused
    assert_reply push(socket, "pass_turn", %{"revision" => 2}), :ok
    current = WebcamTableState.snapshot(room_id)
    assert current.turns.counts == %{alice => 3, bob => 1}
    assert current.turns.active_player_id == alice
    assert current.turns.started_elapsed_ms == WebcamTableState.elapsed(paused, paused.server_now)
    assert current.timer.started_at == first.timer.started_at
    join_player(room_id, "peer-c", "Cara")
    expected = current.turns
    assert_push "table_state", %{turns: ^expected, auto_randomize: false}
    # A reshuffle leaves the active player, counts and pause untouched.
    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-c", "peer-b", "peer-a"]}),
                 :error

    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-b", "peer-a"]}), :ok
    assert WebcamTableState.snapshot(room_id).turns == expected
    assert WebcamTableState.snapshot(room_id).timer.paused_at == paused.paused_at
  end

  test "eliminating skips turns but disconnecting does not advance the game", %{
    socket: socket,
    room_id: room_id,
    player: player
  } do
    other = join_player(room_id, "peer-b", "Bob")
    bob = other.assigns.participant.player_id
    alice = player.id
    assert_reply push(socket, "seat_order", %{"peer_ids" => ["peer-a", "peer-b"]}), :ok
    assert_reply push(socket, "update_status", %{"eliminated" => true}), :ok
    assert_broadcast "table_state", %{turns: %{active_player_id: ^bob, revision: 2}}
    assert_reply push(other, "pass_turn", %{"revision" => 2}), :ok
    assert WebcamTableState.snapshot(room_id).turns.counts == %{alice => 1, bob => 2}
    assert_reply push(socket, "update_status", %{"eliminated" => false}), :ok
    Process.unlink(other.channel_pid)
    ref = Process.monitor(other.channel_pid)
    assert_reply leave(other), :ok
    assert_receive {:DOWN, ^ref, :process, _, _}

    _ = :sys.get_state(WebcamTableState)

    assert %{active_player_id: ^bob, counts: %{^alice => 1, ^bob => 2}, revision: 3} =
             WebcamTableState.snapshot(room_id).turns
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

  test "reload replaces a stale channel at capacity and its DOWN cannot erase the new seat", %{
    socket: original,
    room_id: room,
    player: player
  } do
    assert_reply push(original, "update_status", %{"life" => 23, "poison" => 6}), :ok
    for index <- 2..10, do: join_seat(room, "peer-#{index}")
    Process.unlink(original.channel_pid)
    ref = Process.monitor(original.channel_pid)
    replacement = rejoin(room, player, "new-peer")
    assert_receive {:DOWN, ^ref, :process, _, _}
    _ = :sys.get_state(WebcamTableState)
    assert replacement.assigns.participant.life == 23
    assert replacement.assigns.participant.poison == 6
    assert WebcamTableState.current?(room, player.id, replacement.channel_pid)
    assert length(WebcamTableState.snapshot(room).seats) == 10
    assert_reply push(replacement, "update_status", %{"life" => 22}), :ok

    assert Enum.find(WebcamTableState.snapshot(room).seats, &(&1.player_id == player.id)).life ==
             22
  end

  test "a last-seat disconnect and state-server restart restore the entire mid-game snapshot", %{
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
      "ownerPeerId" => "peer-a",
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
    before = WebcamTableState.snapshot(room)
    disconnect(original)
    assert :ok = Supervisor.terminate_child(TheGathering.Supervisor, WebcamTableState)
    assert {:ok, _} = Supervisor.restart_child(TheGathering.Supervisor, WebcamTableState)
    rejoined = rejoin(room, player, "after-restart")
    after_restart = WebcamTableState.snapshot(room)
    assert Map.drop(after_restart.timer, [:server_now]) == Map.drop(before.timer, [:server_now])
    assert after_restart.turns == before.turns
    assert after_restart.turns.counts == %{player.id => 2}
    assert after_restart.peer_ids == ["after-restart"]
    assert after_restart.monarch.holder.peer_id == "after-restart"
    assert after_restart.cards == [%{card | "ownerPeerId" => "after-restart"}]
    assert after_restart.auto_randomize == false

    assert Map.drop(rejoined.assigns.participant, [:peer_id]) ==
             Map.drop(hd(before.seats), [:peer_id])

    assert_reply push(rejoined, "update_status", %{"eliminated" => true}), :ok
    disconnect(rejoined)
    eliminated = rejoin(room, player, "again")
    assert eliminated.assigns.participant.eliminated
    assert eliminated.assigns.participant.life == 17
    assert [%{peer_id: "again"}] = WebcamTableState.snapshot(room).eliminated_seats
  end

  test "expired disconnected sessions are pruned instead of resurrected", %{
    socket: socket,
    room_id: room,
    player: player
  } do
    alias TheGathering.WebcamTables.Session
    assert_reply push(socket, "update_status", %{"life" => 3}), :ok
    disconnect(socket)

    Repo.update_all(from(s in Session, where: s.id == ^room),
      set: [expires_at: DateTime.add(DateTime.utc_now(), -1, :second)]
    )

    assert Session.load(room) == nil
    assert {1, nil} = Session.prune()
    assert rejoin(room, player, "fresh").assigns.participant.life == 40
  end

  defp disconnect(socket) do
    Process.unlink(socket.channel_pid)
    ref = Process.monitor(socket.channel_pid)
    assert_reply leave(socket), :ok
    assert_receive {:DOWN, ^ref, :process, _, _}
    _ = :sys.get_state(WebcamTableState)
  end

  defp rejoin(room, player, peer) do
    UserSocket
    |> socket(peer, %{user: Accounts.get_user(player.user_id)})
    |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room}", %{
      "peer_id" => peer,
      "player_id" => player.id,
      "protocol" => 2
    })
  end

  test "rejects a player not linked to the authenticated account", %{
    player: player,
    room_id: room_id
  } do
    user = AccountsFixtures.user_fixture()
    socket = socket(UserSocket, "peer-b", %{user: user})

    assert {:error, %{reason: "account is not linked to this player"}} =
             subscribe_and_join(socket, WebcamTableChannel, "webcam_table:#{room_id}", %{
               "peer_id" => "peer-b",
               "player_id" => player.id
             })
  end
end
