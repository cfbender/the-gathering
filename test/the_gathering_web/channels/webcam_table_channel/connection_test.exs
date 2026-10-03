defmodule TheGatheringWeb.WebcamTableChannel.ConnectionTest do
  # Joining a table: socket authentication, SFU media negotiation, signaling, seat
  # admission and presence/status updates.
  use TheGatheringWeb.WebcamTableChannelCase

  alias ExWebRTC.PeerConnection
  alias TheGathering.{Accounts, AccountsFixtures, Games, WebcamTables}
  alias TheGathering.WebcamTables.Sfu
  alias TheGatheringWeb.{Presence, UserSocket, WebcamTableChannel, WebcamTableRooms}

  @peer_a "00000000-0000-4000-8000-00000000000a"
  @peer_b "00000000-0000-4000-8000-00000000000b"
  @peer_c "00000000-0000-4000-8000-00000000000c"

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

  test "joins with a real player and negotiates a media connection with the SFU", %{
    socket: socket,
    room_id: room
  } do
    assert_push "presence_state", %{@peer_a => %{metas: [meta]}}
    assert meta.player_name == "Alice"
    assert [{sfu, _value}] = Registry.lookup(Sfu.Registry, room)

    # The browser's offer carries its camera; the server answers with a matching m-line.
    {browser, offer} = browser_offer()
    ref = push(socket, "sfu_offer", %{"sdp" => offer})
    assert_reply ref, :ok, %{sdp: answer}, 1_000
    assert answer =~ "m=video"
    assert :ok = PeerConnection.set_remote_description(browser, answer(answer))

    # A second seat whose camera arrives at the SFU triggers a server offer to the first.
    # Bob is offered Alice's board at the same time; both sockets push to this process.
    bob = join_player(room, @peer_b, "Bob")
    {_bob_browser, bob_offer} = browser_offer()
    assert_reply push(bob, "sfu_offer", %{"sdp" => bob_offer}), :ok, %{sdp: _answer}, 1_000
    assert_push "sfu_offer", %{sdp: offer_1, tracks: tracks_1}, 1_000
    assert_push "sfu_offer", %{sdp: offer_2, tracks: tracks_2}, 1_000

    assert {server_offer, tracks} =
             Enum.find([{offer_1, tracks_1}, {offer_2, tracks_2}], fn {_sdp, tracks} ->
               @peer_b in Map.values(tracks)
             end)

    assert [{mid, @peer_b}] = Map.to_list(tracks)
    assert server_offer =~ "a=mid:#{mid}"

    # Alice answers; the fake browser produces the answer from the server's offer.
    :ok = PeerConnection.set_remote_description(browser, offer(server_offer))
    {:ok, browser_answer} = PeerConnection.create_answer(browser)
    :ok = PeerConnection.set_local_description(browser, browser_answer)
    assert_reply push(socket, "sfu_answer", %{"sdp" => browser_answer.sdp}), :ok, %{}, 1_000

    # Only one offer may be outstanding; an unsolicited answer is refused.
    assert_reply push(socket, "sfu_answer", %{"sdp" => browser_answer.sdp}),
                 :error,
                 %{reason: "answer rejected"},
                 1_000

    # Candidates are accepted before and after the answer and never fan out to the room.
    candidate = %{"candidate" => "candidate:1 1 udp 1 127.0.0.1 9 typ host", "sdpMid" => "0"}
    push(socket, "sfu_candidate", %{"candidate" => candidate})
    refute_broadcast "sfu_candidate", _

    assert_reply push(socket, "sfu_candidate", %{"candidate" => %{}}), :error, %{
      reason: "invalid candidate"
    }

    # The room monitors each channel and closes once every seat has left.
    monitor = Process.monitor(sfu)
    Process.unlink(socket.channel_pid)
    Process.unlink(bob.channel_pid)
    close(socket)
    close(bob)
    assert_receive {:DOWN, ^monitor, :process, ^sfu, :normal}
  end

  test "answers a failed media connection with an ICE restart before giving up on the seat",
       %{socket: socket, room_id: room} do
    assert [{sfu, _value}] = Registry.lookup(Sfu.Registry, room)
    {browser, offer} = browser_offer()
    assert_reply push(socket, "sfu_offer", %{"sdp" => offer}), :ok, %{sdp: answer}, 1_000
    :ok = PeerConnection.set_remote_description(browser, answer(answer))
    [ufrag] = Regex.run(~r/a=ice-ufrag:(\S+)/, answer, capture: :all_but_first)
    pc = :sys.get_state(sfu).peers[@peer_a].pc

    # The first failures re-offer with fresh ICE credentials and keep the channel open.
    restart_ufrags =
      for _attempt <- 1..3 do
        send(sfu, {:ex_webrtc, pc, {:connection_state_change, :failed}})
        assert_push "sfu_offer", %{sdp: restart_offer, tracks: %{}}, 1_000
        [restart_ufrag] = Regex.run(~r/a=ice-ufrag:(\S+)/, restart_offer, capture: :all_but_first)
        refute restart_ufrag == ufrag

        :ok = PeerConnection.set_remote_description(browser, offer(restart_offer))
        {:ok, browser_answer} = PeerConnection.create_answer(browser)
        :ok = PeerConnection.set_local_description(browser, browser_answer)
        assert_reply push(socket, "sfu_answer", %{"sdp" => browser_answer.sdp}), :ok, %{}, 1_000
        restart_ufrag
      end

    assert length(Enum.uniq(restart_ufrags)) == 3
    assert Process.alive?(socket.channel_pid)

    # A fourth failure within the window drops the seat so the browser rejoins afresh.
    Process.unlink(socket.channel_pid)
    monitor = Process.monitor(socket.channel_pid)
    send(sfu, {:ex_webrtc, pc, {:connection_state_change, :failed}})
    assert_receive {:DOWN, ^monitor, :process, _pid, {:sfu_down, :failed}}, 1_000
    refute Map.has_key?(:sys.get_state(sfu).peers, @peer_a)
  end

  test "rejects oversize or malformed offers and answers", %{socket: socket} do
    oversize = String.duplicate("a", 65_537)

    assert_reply push(socket, "sfu_offer", %{"sdp" => oversize}), :error, %{
      reason: "invalid offer"
    }

    assert_reply push(socket, "sfu_offer", %{"sdp" => "not sdp"}),
                 :error,
                 %{reason: "offer rejected"},
                 1_000

    # The server has not offered, so there is nothing to answer.
    assert_reply push(socket, "sfu_answer", %{"sdp" => "v=0\r\n"}),
                 :error,
                 %{reason: "answer rejected"},
                 1_000
  end

  test "relays a message to one other seat without broadcasting it", %{
    socket: socket,
    room_id: room
  } do
    bob = join_player(room, @peer_b, "Bob")
    assert_push "presence_diff", %{joins: %{@peer_b => _}}

    push(socket, "peer_message", %{"to" => @peer_b, "message" => %{"type" => "crop"}})
    assert_push "peer_message", %{from: @peer_a, message: %{"type" => "crop"}}
    refute_broadcast "peer_message", _

    assert_reply push(socket, "peer_message", %{"to" => @peer_a, "message" => %{}}), :error, %{
      reason: "invalid recipient"
    }

    assert_reply push(socket, "peer_message", %{"to" => @peer_c, "message" => %{}}), :error, %{
      reason: "recipient has left"
    }

    oversize = %{"data" => String.duplicate("a", 262_145)}

    assert_reply push(socket, "peer_message", %{"to" => @peer_b, "message" => oversize}),
                 :error,
                 %{reason: "message too large"}

    Process.unlink(bob.channel_pid)
    close(bob)
  end

  test "validates layer requests against the known layers and seats", %{socket: socket} do
    assert_reply push(socket, "sfu_layer", %{"peer_id" => @peer_b, "layer" => "xl"}), :error, %{
      reason: "invalid layer"
    }

    assert_reply push(socket, "sfu_layer", %{"peer_id" => @peer_b, "layer" => "l"}), :error, %{
      reason: "unknown board"
    }
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

  test "publishes the camera's native height and correction consent for viewer-side crops", %{
    socket: socket,
    room_id: room_id
  } do
    assert_push "presence_state", %{@peer_a => %{metas: [meta]}}
    assert %{camera_height: nil, shares_corrections: false} = meta

    assert_reply push(socket, "update_status", %{
                   "camera_height" => 1080,
                   "shares_corrections" => true
                 }),
                 :ok

    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{camera_height: 1080, shares_corrections: true} = meta

    for bad <- [
          %{"camera_height" => 0},
          %{"camera_height" => "1080"},
          %{"shares_corrections" => 1}
        ] do
      assert_reply push(socket, "update_status", bad), :error, %{reason: "invalid status"}
    end

    # The placeholder stream has no camera behind it.
    assert_reply push(socket, "update_status", %{"camera_height" => nil}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", @peer_a)
    assert %{camera_height: nil, shares_corrections: true} = meta
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

  test "rejects invalid room IDs", %{player: player} do
    user = AccountsFixtures.user_fixture()
    socket = socket(UserSocket, @peer_b, %{user: user})

    assert {:error, %{reason: "invalid room"}} =
             subscribe_and_join(socket, WebcamTableChannel, "webcam_table:not-a-uuid", %{
               "peer_id" => @peer_b,
               "player_id" => player.id
             })
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
        push(bob, "peer_message", %{"to" => @peer_a, "message" => %{"type" => "crop"}})
        assert_push "peer_message", %{from: @peer_b}
      end

      assert_reply push(bob, "peer_message", %{"to" => @peer_a, "message" => %{}}), :error, %{
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
