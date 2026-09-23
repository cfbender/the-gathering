defmodule TheGatheringWeb.WebcamTableChannelTest do
  use TheGathering.DataCase, async: false
  import Phoenix.ChannelTest

  alias TheGathering.{Accounts, AccountsFixtures, Games}
  alias TheGatheringWeb.{Presence, UserSocket, WebcamTableChannel}

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
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert meta.deck_id == deck.id

    {:ok, other} = Games.create_player(%{name: "Bob"})

    {:ok, other_deck} =
      Games.create_deck(%{player_id: other.id, name: "Dragons", commander_name: "Miirym"})

    assert_reply push(socket, "choose_deck", %{"deck_id" => other_deck.id}), :error, %{
      reason: "deck does not belong to player"
    }
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

  test "monarch is one shared holder, synchronized to late joiners and cleared on departure", %{
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
    assert_broadcast "monarch", %{holder: nil}
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
    assert snapshot_revision > last_revision
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
