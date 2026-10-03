defmodule TheGatheringWeb.WebcamTableChannel.GameModesTest do
  # Game modes and table controls: owner/admin controls, Two-Headed Giant and
  # start-of-game seat randomization.
  use TheGatheringWeb.WebcamTableChannelCase

  alias TheGathering.{AccountsFixtures, Games, WebcamTables}
  alias TheGathering.WebcamTables.Session
  alias TheGatheringWeb.{UserSocket, WebcamTableChannel}

  @peer_a "00000000-0000-4000-8000-00000000000a"
  @peer_b "00000000-0000-4000-8000-00000000000b"
  @peer_c "00000000-0000-4000-8000-00000000000c"
  @peer_d "00000000-0000-4000-8000-00000000000d"
  @peer_e "00000000-0000-4000-8000-00000000000e"
  @peer_f "00000000-0000-4000-8000-00000000000f"
  @mate_returned "00000000-0000-4000-8000-00000000b0b2"
  @spectator_peer "00000000-0000-4000-8000-000000005bec"

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

  test "admins hold table controls in rooms they did not open", %{room_id: room} do
    join_admin = fn peer_id ->
      user = AccountsFixtures.admin_fixture()
      {:ok, player} = Games.create_player(%{name: "Admin #{peer_id}"}, user.id)

      {:ok, reply, socket} =
        UserSocket
        |> socket(peer_id, %{user: user})
        |> subscribe_and_join(WebcamTableChannel, "webcam_table:#{room}", %{
          "peer_id" => peer_id,
          "player_id" => player.id
        })

      {reply, socket}
    end

    {%{owner: true}, admin} = join_admin.(@peer_b)
    member = join_seat(room, @peer_c)
    join_seat(room, @peer_d)

    assert_reply push(member, "set_mode", %{"mode" => "two_headed_giant"}), :error
    assert_reply push(admin, "set_mode", %{"mode" => "two_headed_giant"}), :ok

    # Teams are adjacent pairs: Alice and C, then the admin and D.
    peers = [@peer_a, @peer_c, @peer_b, @peer_d]
    assert_reply push(admin, "arrange_seats", %{"peer_ids" => peers}), :ok
    assert_reply push(admin, "turn_settings", %{"auto_randomize" => false}), :ok
    assert_reply push(admin, "start_game", %{}), :ok

    assert_reply push(member, "adjust_team_life", %{"team_index" => 1, "delta" => -1}), :error
    assert_reply push(admin, "adjust_team_life", %{"team_index" => 0, "delta" => -1}), :ok
    assert WebcamTables.snapshot(room).team_life[0] == 59

    assert_reply push(admin, "set_eliminated", %{"peer_id" => @peer_c, "eliminated" => true}),
                 :ok

    # A late admin spectates, and spectators never hold table controls.
    {%{owner: false}, spectator} = join_admin.(@spectator_peer)
    assert_reply push(spectator, "timer", %{"action" => "pause"}), :error
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
end
