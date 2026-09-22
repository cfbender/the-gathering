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
    assert %{life: 40, muted: true, camera_off: false, joined_at: joined_at} = meta
    assert is_integer(joined_at)

    assert_reply push(socket, "update_status", %{"life" => 37, "camera_off" => true}), :ok
    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert %{life: 37, camera_off: true, muted: true} = meta

    assert_reply push(socket, "update_status", %{"life" => 1_000}), :error, %{
      reason: "invalid status"
    }

    assert_reply push(socket, "update_status", %{"life" => 20, "role" => "admin"}), :error, %{
      reason: "invalid status"
    }

    %{metas: [meta]} = Presence.get_by_key("webcam_table:#{room_id}", "peer-a")
    assert meta.life == 37
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
