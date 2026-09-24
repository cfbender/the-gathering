defmodule TheGatheringWeb.WebcamTableChannelTest do
  use TheGathering.DataCase, async: false
  import Phoenix.ChannelTest

  alias TheGathering.{Accounts, AccountsFixtures, Games, WebcamTables}
  alias TheGathering.WebcamTables.{Room, Session, Timer}
  alias TheGatheringWeb.{Presence, UserSocket, WebcamTableChannel, WebcamTableRooms}

  @endpoint TheGatheringWeb.Endpoint

  # Clients generate peer IDs with crypto.randomUUID(), and the channel only
  # accepts canonical UUIDs.
  @peer_a "00000000-0000-4000-8000-00000000000a"
  @peer_b "00000000-0000-4000-8000-00000000000b"
  @peer_c "00000000-0000-4000-8000-00000000000c"
  @peer_d "00000000-0000-4000-8000-00000000000d"
  @peer_e "00000000-0000-4000-8000-00000000000e"
  @peer_f "00000000-0000-4000-8000-00000000000f"
  @peer_a_new "00000000-0000-4000-8000-0000000000a2"
  @new_peer "00000000-0000-4000-8000-00000000a0a0"
  @after_restart "00000000-0000-4000-8000-00000000a0a1"
  @again "00000000-0000-4000-8000-00000000a0a2"
  @mate_returned "00000000-0000-4000-8000-00000000b0b2"
  @fresh "00000000-0000-4000-8000-00000000f0f0"
  @elsewhere "00000000-0000-4000-8000-00000000e0e0"
  @spectator_peer "00000000-0000-4000-8000-000000005bec"

  defp peer(index),
    do: "00000000-0000-4000-8000-" <> String.pad_leading(Integer.to_string(index), 12, "0")

  test "socket connection uses the tracked cookie session" do
    user = AccountsFixtures.user_fixture()
    session_token = Accounts.generate_user_session_token(user)
    token = UserSocket.token(TheGatheringWeb.Endpoint, session_token)
    socket = socket(UserSocket, nil, %{})

    assert {:ok, connected} = UserSocket.connect(%{"token" => token}, socket, %{})

    assert connected.assigns.user.id == user.id
    assert UserSocket.id(connected) == "users_sessions:#{Base.url_encode64(session_token)}"
    assert :error = UserSocket.connect(%{"token" => "invalid"}, socket, %{})

    # Logging out deletes the session token, so its socket token stops working.
    Accounts.delete_user_session_token(session_token)
    assert :error = UserSocket.connect(%{"token" => token}, socket, %{})
  end

  test "socket tokens are encrypted, and tampered or merely signed tokens are rejected" do
    user = AccountsFixtures.user_fixture()
    session_token = Accounts.generate_user_session_token(user)
    token = UserSocket.token(TheGatheringWeb.Endpoint, session_token)
    socket = socket(UserSocket, nil, %{})

    refute token =~ Base.url_encode64(session_token, padding: false)
    refute token =~ Base.encode64(session_token, padding: false)

    # Flip a middle character; trailing base64 characters can sit in padding bits.
    middle = token |> String.length() |> div(2)

    middle =
      Enum.find(middle..String.length(token), &(String.at(token, &1) not in [".", "-", "_"]))

    flipped = if String.at(token, middle) == "A", do: "B", else: "A"

    for tampered <- [
          String.slice(token, 0, middle) <> flipped <> String.slice(token, (middle + 1)..-1//1),
          String.slice(token, 0, middle),
          Phoenix.Token.sign(TheGatheringWeb.Endpoint, "webcam table socket", session_token)
        ] do
      assert :error = UserSocket.connect(%{"token" => tampered}, socket, %{})
    end

    assert :error = UserSocket.connect(%{"token" => 123}, socket, %{})
  end

  setup do
    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: "Alice"}, user.id)

    {:ok, deck} =
      Games.create_deck(%{player_id: player.id, name: "Birds", commander_name: "Kangee"})

    room_id = Ecto.UUID.generate()

    socket =
      UserSocket
      |> socket(@peer_a, %{user: user})
      |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room_id}", %{
        "peer_id" => @peer_a,
        "player_id" => player.id
      })

    %{socket: socket, player: player, deck: deck, room_id: room_id}
  end

  test "joins with a real player and relays targeted signaling", %{socket: socket} do
    assert_push "presence_state", %{@peer_a => %{metas: [meta]}}
    assert meta.player_name == "Alice"

    push(socket, "signal", %{"target" => @peer_b, "signal" => %{"candidate" => "ice"}})

    assert_broadcast "signal", %{
      target: @peer_b,
      from: @peer_a,
      signal: %{"candidate" => "ice"}
    }
  end

  test "rejects oversize signals and non-peer targets without relaying them", %{socket: socket} do
    # A realistic SDP offer is well under the cap.
    offer = %{"type" => "offer", "sdp" => String.duplicate("a=candidate:x\r\n", 1_500)}
    push(socket, "signal", %{"target" => @peer_b, "signal" => offer})
    assert_broadcast "signal", %{target: @peer_b, signal: ^offer}

    oversize = %{"sdp" => String.duplicate("a", 65_537)}

    assert_reply push(socket, "signal", %{"target" => @peer_b, "signal" => oversize}),
                 :error,
                 %{reason: "signal too large"}

    assert_reply push(socket, "signal", %{"target" => "peer-b", "signal" => %{}}), :error, %{
      reason: "invalid signal"
    }

    refute_broadcast "signal", _
  end

  test "the websocket caps inbound frames above the largest legitimate signal" do
    [{"/socket", TheGatheringWeb.UserSocket, opts}] = TheGatheringWeb.Endpoint.__sockets__()
    max_frame_size = opts[:websocket][:max_frame_size]
    assert is_integer(max_frame_size)
    assert max_frame_size > 65_536
  end

  test "updates presence only with a deck owned by the seated player", %{
    socket: socket,
    deck: deck,
    room_id: room_id
  } do
    assert_reply push(socket, "choose_deck", %{"deck_id" => deck.id}), :ok
    deck_id = deck.id
    assert_broadcast "deck_selected", %{deck_id: ^deck_id}
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
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
    assert_push "presence_state", %{@peer_a => %{metas: [meta]}}
    assert %{life: 40, camera_off: false, joined_at: joined_at} = meta
    assert is_integer(joined_at)

    assert_reply push(socket, "update_status", %{"life" => 37, "camera_off" => true}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{life: 37, camera_off: true} = meta
    refute Map.has_key?(meta, :muted)

    assert_reply push(socket, "update_status", %{"life" => 1_000}), :error, %{
      reason: "invalid status"
    }

    assert_reply push(socket, "update_status", %{"life" => 20, "role" => "admin"}), :error, %{
      reason: "invalid status"
    }

    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert meta.life == 37
  end

  test "a full status update right after joining is applied", %{socket: socket, room_id: room} do
    full = %{
      "life" => 33,
      "poison" => 1,
      "rad" => 2,
      "commander_casts" => %{},
      "commander_damage" => %{},
      "camera_off" => true
    }

    assert_reply push(socket, "update_status", full), :ok
    assert %{metas: [%{life: 33, camera_off: true}]} = Presence.get_by_key(socket.topic, @peer_a)
    assert Enum.map(WebcamTables.snapshot(room).seats, & &1.life) == [33]
  end

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
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a]}), :ok
    assert_broadcast "seat_order", %{peer_ids: [@peer_a]}

    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a, "peer-ghost"]}),
                 :error,
                 %{reason: "seat order must list every seated player"}

    assert_reply push(socket, "seat_order", %{"peer_ids" => @peer_a}), :error, %{
      reason: "invalid seat order"
    }
  end

  test "rejects invalid room IDs", %{player: player} do
    user = AccountsFixtures.user_fixture()
    socket = socket(UserSocket, @peer_b, %{user: user})

    assert {:error, %{reason: "invalid room"}} =
             subscribe_and_join(socket, WebcamTableChannel, "webcam_table:not-a-uuid", %{
               "peer_id" => @peer_b,
               "player_id" => player.id
             })
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

  test "admits ten seats, marks the lobby full, and refuses the eleventh", %{
    socket: socket,
    room_id: room_id
  } do
    for index <- 2..9, do: join_seat(room_id, peer(index))
    refute Enum.find(WebcamTableRooms.active_rooms(), &(&1.id == room_id)).full
    join_seat(room_id, peer(10))
    assert map_size(Presence.list(socket)) == 10
    assert Enum.find(WebcamTableRooms.active_rooms(), &(&1.id == room_id)).full

    {user, player} = linked_player(peer(11))

    assert {:error, %{reason: "room is full"}} =
             UserSocket
             |> socket(peer(11), %{user: user})
             |> subscribe_and_join(WebcamTableChannel, socket.topic, %{
               "peer_id" => peer(11),
               "player_id" => player.id
             })

    order = [@peer_a | Enum.map(2..10, &peer(&1))]
    assert_reply push(socket, "seat_order", %{"peer_ids" => Enum.reverse(order)}), :ok
    assert_broadcast "seat_order", %{peer_ids: peer_ids}
    assert peer_ids == Enum.reverse(order)
  end

  test "rejects non-UUID and duplicate peer IDs", %{socket: seated} do
    {user, player} = linked_player("Bob")

    for {peer_id, reason} <- [
          {"", "invalid peer id"},
          {"peer-b", "invalid peer id"},
          {String.upcase(@peer_b), "invalid peer id"},
          {String.duplicate("a", 10_000), "invalid peer id"},
          {123, "invalid peer id"},
          {@peer_a, "peer id is already in use"}
        ] do
      assert {:error, %{reason: ^reason}} =
               UserSocket
               |> socket(nil, %{user: user})
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
    assert_reply push(socket, "seat_order", %{"peer_ids" => [@peer_a]}), :ok
    assert_broadcast "timer_state", %{started_at: started, paused_at: nil, paused_ms: 0}
    assert started >= before_start
    assert started <= System.system_time(:millisecond)

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
    assert_reply push(other, "pass_turn", %{"revision" => 1}), :ok
    assert_reply push(socket, "pass_turn", %{"revision" => 1}), :error

    assert_broadcast "table_state", %{
      turns: %{active_player_id: ^bob, counts: %{^alice => 1, ^bob => 1}, revision: 2}
    }

    assert_reply push(socket, "adjust_turn", %{"player_id" => alice, "delta" => 1}), :ok
    assert WebcamTables.snapshot(room_id).turns.counts[alice] == 2
    assert_reply push(socket, "timer", %{"action" => "pause"}), :ok, paused
    assert_reply push(socket, "pass_turn", %{"revision" => 2}), :ok
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

  test "a last-seat disconnect stops the room and rejoining restores the entire mid-game snapshot",
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
    room_ref = Process.monitor(room_pid(room))
    disconnect(original)
    # The room process stops with its last connection; the session survives it.
    assert_receive {:DOWN, ^room_ref, :process, _, :normal}
    assert room_pid(room) == nil
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
    assert rejoin(room, player, @fresh).assigns.participant.life == 40
  end

  test "mode is owner-only, validates roster, and freezes order after start", %{
    socket: owner,
    room_id: room
  } do
    other = join_seat(room, @peer_b)
    assert_reply push(other, "set_mode", %{"mode" => "five_star"}), :error
    assert_reply push(owner, "set_mode", %{"mode" => "invalid"}), :error
    assert_reply push(owner, "set_mode", %{"mode" => "five_star"}), :ok

    assert_reply push(owner, "start_game", %{}), :error, %{
      reason: "Five Star requires exactly 5 players"
    }

    for peer <- [@peer_c, @peer_d], do: join_seat(room, peer)
    assert_reply push(owner, "start_game", %{}), :error
    join_seat(room, @peer_e)
    peers = [@peer_e, @peer_a, @peer_c, @peer_b, @peer_d]
    assert_reply push(other, "arrange_seats", %{"peer_ids" => peers}), :error
    assert_reply push(owner, "arrange_seats", %{"peer_ids" => peers}), :ok
    assert WebcamTables.snapshot(room).timer.started_at == nil
    assert_reply push(owner, "turn_settings", %{"auto_randomize" => false}), :ok
    assert_reply push(owner, "start_game", %{}), :ok
    assert WebcamTables.snapshot(room).peer_ids == peers
    assert_reply push(owner, "set_mode", %{"mode" => "commander"}), :error
    assert_reply push(owner, "seat_order", %{"peer_ids" => Enum.reverse(peers)}), :error
    assert_reply push(owner, "arrange_seats", %{"peer_ids" => Enum.reverse(peers)}), :error
    assert WebcamTables.snapshot(room).peer_ids == peers
    spectator = join_seat(room, @spectator_peer)
    assert_reply push(spectator, "set_mode", %{"mode" => "commander"}), :error
  end

  test "2HG validates teams, serializes shared life and eliminates offline teammates", %{
    socket: owner,
    room_id: room,
    player: alice
  } do
    mate = join_seat(room, @peer_b)
    assert_reply push(owner, "set_mode", %{"mode" => "two_headed_giant"}), :ok
    assert_reply push(owner, "start_game", %{}), :error
    rival = join_seat(room, @peer_c)
    assert_reply push(owner, "start_game", %{}), :error
    join_seat(room, @peer_d)

    assert_reply push(owner, "arrange_seats", %{
                   "peer_ids" => [@peer_a, @peer_b, @peer_c, @peer_d]
                 }),
                 :ok

    assert_reply push(owner, "turn_settings", %{"auto_randomize" => false}), :ok
    assert_reply push(owner, "adjust_team_life", %{"team_index" => 0, "delta" => 1}), :error
    assert_reply push(owner, "start_game", %{}), :ok
    assert WebcamTables.snapshot(room).team_life == %{0 => 60, 1 => 60}
    assert_reply push(mate, "adjust_team_life", %{"team_index" => 0, "delta" => -7}), :ok
    assert_reply push(owner, "adjust_team_life", %{"team_index" => 0, "delta" => 2}), :ok
    assert WebcamTables.snapshot(room).team_life[0] == 55
    assert_reply push(rival, "adjust_team_life", %{"team_index" => 0, "delta" => -1}), :error
    assert_reply push(owner, "adjust_team_life", %{"team_index" => 1, "delta" => 1998}), :ok
    assert WebcamTables.snapshot(room).team_life[1] == 999
    assert_reply push(owner, "adjust_team_life", %{"team_index" => 1, "delta" => -1998}), :ok
    assert WebcamTables.snapshot(room).team_life[1] == -999

    # Zero shared life knocks out the whole team, but only that team.
    assert_broadcast "eliminated_seats", %{participants: knocked_out}
    assert knocked_out |> Enum.map(& &1.peer_id) |> Enum.sort() == [@peer_c, @peer_d]

    assert Enum.all?(
             WebcamTables.snapshot(room).seats,
             &(&1.eliminated == &1.peer_id in [@peer_c, @peer_d])
           )

    # Gaining life back does not restore; the owner restores the team explicitly.
    assert_reply push(owner, "adjust_team_life", %{"team_index" => 1, "delta" => 1000}), :ok
    assert WebcamTables.snapshot(room).eliminated_seats |> length() == 2

    assert_reply push(owner, "set_eliminated", %{"peer_id" => @peer_c, "eliminated" => false}),
                 :ok

    assert WebcamTables.snapshot(room).eliminated_seats == []
    assert_reply push(owner, "adjust_team_life", %{"team_index" => 1, "delta" => -1000}), :ok
    assert WebcamTables.snapshot(room).team_life[1] == -999

    assert_reply push(owner, "set_eliminated", %{"peer_id" => @peer_d, "eliminated" => false}),
                 :ok

    assert WebcamTables.snapshot(room).eliminated_seats == []

    for payload <- [
          %{"team_index" => -1, "delta" => 1},
          %{"team_index" => 9, "delta" => 1},
          %{"team_index" => 0, "delta" => 1.5}
        ] do
      assert_reply push(owner, "adjust_team_life", payload), :error
    end

    assert_reply push(owner, "adjust_turn", %{
                   "player_id" => mate.assigns.participant.player_id,
                   "delta" => 1
                 }),
                 :ok

    assert WebcamTables.snapshot(room).turns.counts == %{alice.id => 2}
    assert_reply push(mate, "pass_turn", %{"revision" => 1}), :ok

    assert WebcamTables.snapshot(room).turns.active_player_id ==
             rival.assigns.participant.player_id

    disconnect(mate)

    assert_reply push(owner, "set_eliminated", %{"peer_id" => @peer_a, "eliminated" => true}),
                 :ok

    assert WebcamTables.snapshot(room).eliminated_seats
           |> Enum.map(& &1.peer_id)
           |> Enum.sort() == [@peer_a, @peer_b]

    assert_reply push(owner, "update_status", %{"life" => 25}), :ok
    restored = rejoin(room, Games.get_player(mate.assigns.participant.player_id), @mate_returned)
    assert restored.assigns.participant.eliminated
    assert_reply push(restored, "update_status", %{"eliminated" => false}), :ok
    assert WebcamTables.snapshot(room).eliminated_seats == []
    assert_reply push(owner, "start_game", %{}), :ok
    assert WebcamTables.snapshot(room).team_life == %{0 => 55, 1 => -999}
    saved = Session.load(room)
    assert saved.mode == "two_headed_giant"
    assert saved.team_life == %{0 => 55, 1 => -999}
    assert saved.turns == WebcamTables.snapshot(room).turns
  end

  test "2HG randomizes whole pairs, and older snapshot formats start fresh", %{
    socket: owner,
    room_id: room
  } do
    for peer <- [@peer_b, @peer_c, @peer_d, @peer_e], do: join_seat(room, peer)
    assert_reply push(owner, "set_mode", %{"mode" => "two_headed_giant"}), :ok
    assert_reply push(owner, "start_game", %{}), :error
    join_seat(room, @peer_f)
    peers = [@peer_d, @peer_a, @peer_f, @peer_b, @peer_e, @peer_c]
    assert_reply push(owner, "arrange_seats", %{"peer_ids" => peers}), :ok
    assert_reply push(owner, "start_game", %{}), :ok

    assert Enum.sort(Enum.chunk_every(WebcamTables.snapshot(room).peer_ids, 2)) ==
             Enum.sort(Enum.chunk_every(peers, 2))

    persisted = Repo.get!(Session, room)
    legacy = persisted.snapshot |> Jason.decode!() |> Map.put("version", 1)
    persisted |> Ecto.Changeset.change(snapshot: Jason.encode!(legacy)) |> Repo.update!()
    assert Session.load(room) == nil
  end

  test "start_game with randomize: false keeps the arranged order despite auto-randomize", %{
    socket: owner,
    room_id: room
  } do
    for peer <- [@peer_b, @peer_c, @peer_d], do: join_seat(room, peer)
    peers = [@peer_d, @peer_a, @peer_b, @peer_c]
    assert_reply push(owner, "arrange_seats", %{"peer_ids" => peers}), :ok
    assert_reply push(owner, "start_game", %{"randomize" => "false"}), :error
    assert_reply push(owner, "start_game", %{"randomize" => false, "extra" => 1}), :error
    assert WebcamTables.snapshot(room).timer.started_at == nil
    assert WebcamTables.snapshot(room).auto_randomize

    assert_reply push(owner, "start_game", %{"randomize" => false}), :ok
    assert_broadcast "seat_order", %{peer_ids: ^peers, shuffled: false}
    assert WebcamTables.snapshot(room).peer_ids == peers
    assert WebcamTables.snapshot(room).timer.started_at != nil
  end

  test "start_game with randomize: true shuffles even when auto-randomize is off", %{
    socket: owner,
    room_id: room
  } do
    for peer <- [@peer_b, @peer_c, @peer_d], do: join_seat(room, peer)
    peers = [@peer_d, @peer_a, @peer_b, @peer_c]
    assert_reply push(owner, "arrange_seats", %{"peer_ids" => peers}), :ok
    assert_reply push(owner, "turn_settings", %{"auto_randomize" => false}), :ok
    assert_reply push(owner, "start_game", %{"randomize" => true}), :ok
    assert_broadcast "seat_order", %{peer_ids: shuffled, shuffled: true}
    assert Enum.sort(shuffled) == Enum.sort(peers)
    assert WebcamTables.snapshot(room).peer_ids == shuffled
  end

  defp disconnect(socket) do
    Process.unlink(socket.channel_pid)
    ref = Process.monitor(socket.channel_pid)
    assert_reply leave(socket), :ok
    assert_receive {:DOWN, ^ref, :process, _, _}
    sync_room(socket.assigns.room_id)
  end

  # Unlike Registry.lookup/2, whereis skips a registered pid that has already exited.
  defp room_pid(room), do: GenServer.whereis(Room.via(room))

  # Waits until the room has handled earlier messages, such as a channel's DOWN.
  # The room may stop meanwhile if that was its last connection.
  defp sync_room(room) do
    if pid = room_pid(room) do
      try do
        :sys.get_state(pid)
      catch
        :exit, _stopped -> :ok
      end
    end

    :ok
  end

  defp rejoin(room, player, peer) do
    UserSocket
    |> socket(peer, %{user: Accounts.get_user(player.user_id)})
    |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room}", %{
      "peer_id" => peer,
      "player_id" => player.id
    })
  end

  describe "rate limits" do
    defp put_rate_limits(overrides) do
      previous = Application.fetch_env!(:the_gathering, TheGatheringWeb.RateLimit)

      Application.put_env(
        :the_gathering,
        TheGatheringWeb.RateLimit,
        Keyword.merge(previous, overrides)
      )

      on_exit(fn -> Application.put_env(:the_gathering, TheGatheringWeb.RateLimit, previous) end)
    end

    test "each connection's events are limited, with signals budgeted separately", %{
      socket: alice,
      room_id: room
    } do
      put_rate_limits(
        webcam_table_events: [capacity: 3, refill_per_second: 0],
        webcam_table_signals: [capacity: 2, refill_per_second: 0]
      )

      bob = join_player(room, @peer_b, "Bob")

      for life <- 39..37//-1,
          do: assert_reply(push(bob, "update_status", %{"life" => life}), :ok)

      assert_reply push(bob, "update_status", %{"life" => 1}), :error, %{reason: "rate limited"}
      assert_reply push(bob, "roll", %{"kind" => "coin"}), :error, %{reason: "rate limited"}
      assert Enum.find(WebcamTables.snapshot(room).seats, &(&1.peer_id == @peer_b)).life == 37

      for _ <- 1..2 do
        push(bob, "signal", %{"target" => @peer_a, "signal" => %{"candidate" => "ice"}})
        assert_broadcast "signal", %{from: @peer_b}
      end

      assert_reply push(bob, "signal", %{"target" => @peer_a, "signal" => %{}}), :error, %{
        reason: "rate limited"
      }

      # Alice joined under the default limits and keeps her own budget.
      assert_reply push(alice, "update_status", %{"life" => 12}), :ok
    end

    test "joins are limited per user so rejoining cannot reset the budget", %{room_id: room} do
      put_rate_limits(webcam_table_joins: [limit: 1, scale: :timer.minutes(1)])
      {user, player} = linked_player("Bob")
      # SQLite reuses rolled-back user IDs, so clear any earlier test's count.
      TheGathering.RateLimiter.set({:webcam_table_joins, user.id}, :timer.minutes(1), 0)
      params = %{"peer_id" => @peer_b, "player_id" => player.id}
      socket = socket(UserSocket, nil, %{user: user})

      assert {:ok, _reply, joined} =
               subscribe_and_join(socket, WebcamTableChannel, "webcam_table:#{room}", params)

      Process.unlink(joined.channel_pid)
      leave(joined)

      assert {:error, %{reason: "rate limited"}} =
               subscribe_and_join(socket, WebcamTableChannel, "webcam_table:#{room}", params)
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

  test "rejects a player not linked to the authenticated account", %{
    player: player,
    room_id: room_id
  } do
    user = AccountsFixtures.user_fixture()
    socket = socket(UserSocket, @peer_b, %{user: user})

    assert {:error, %{reason: "account is not linked to this player"}} =
             subscribe_and_join(socket, WebcamTableChannel, "webcam_table:#{room_id}", %{
               "peer_id" => @peer_b,
               "player_id" => player.id
             })
  end
end
