defmodule TheGatheringWeb.WebcamTableChannelCase do
  @moduledoc """
  Shared setup and helpers for the webcam table channel tests.

  Every peer connection the SFU starts generates an RSA DTLS certificate, which makes these
  tests the slowest in the suite. They are split across several files under
  `test/the_gathering_web/channels/webcam_table_channel/` so `mix test --partitions` can run
  them in parallel. Each test starts with Alice seated as `peer_a` in a fresh room.
  """

  use ExUnit.CaseTemplate

  import Phoenix.ChannelTest

  alias ExWebRTC.{PeerConnection, SessionDescription}
  alias TheGathering.{Accounts, AccountsFixtures, Games}
  alias TheGathering.WebcamTables.Room
  alias TheGatheringWeb.{UserSocket, WebcamTableChannel}

  @endpoint TheGatheringWeb.Endpoint

  # Clients generate peer IDs with crypto.randomUUID(), and the channel only
  # accepts canonical UUIDs.
  @peer_a "00000000-0000-4000-8000-00000000000a"

  using do
    quote do
      @endpoint TheGatheringWeb.Endpoint

      # The same conveniences as `use TheGathering.DataCase`.
      alias TheGathering.Repo

      import Ecto
      import Ecto.Changeset
      import Ecto.Query
      import TheGathering.DataCase

      import Phoenix.ChannelTest
      import TheGatheringWeb.WebcamTableChannelCase
    end
  end

  setup tags do
    TheGathering.DataCase.setup_sandbox(tags)

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

  def peer(index),
    do: "00000000-0000-4000-8000-" <> String.pad_leading(Integer.to_string(index), 12, "0")

  # What a browser sends the SFU first: an offer with one video camera track.
  def browser_offer do
    {:ok, pc} = PeerConnection.start_link(video_codecs: [:h264, :vp8], audio_codecs: [])
    {:ok, _transceiver} = PeerConnection.add_transceiver(pc, :video, direction: :sendonly)
    {:ok, offer} = PeerConnection.create_offer(pc)
    :ok = PeerConnection.set_local_description(pc, offer)
    {pc, offer.sdp}
  end

  def offer(sdp), do: %SessionDescription{type: :offer, sdp: sdp}

  def answer(sdp), do: %SessionDescription{type: :answer, sdp: sdp}

  def join_player(room_id, peer_id, name) do
    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: name}, user.id)

    UserSocket
    |> socket(peer_id, %{user: user})
    |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room_id}", %{
      "peer_id" => peer_id,
      "player_id" => player.id
    })
  end

  def linked_player(name) do
    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: name}, user.id)
    {user, player}
  end

  def join_seat(room_id, peer_id) do
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

  def disconnect(socket) do
    Process.unlink(socket.channel_pid)
    ref = Process.monitor(socket.channel_pid)
    assert_reply leave(socket), :ok
    assert_receive {:DOWN, ^ref, :process, _, _}
    sync_room(socket.assigns.room_id)
  end

  # Unlike Registry.lookup/2, whereis skips a registered pid that has already exited.
  def room_pid(room), do: GenServer.whereis(Room.via(room))

  # The registry removes a stopped room's entry when its (single) partition
  # handles the room's exit; suspending the partition holds that cleanup.
  def with_registry_cleanup_paused(fun) do
    partition = Module.concat(TheGathering.WebcamTables.Registry, "PIDPartition0")
    :ok = :sys.suspend(partition)

    try do
      fun.()
    after
      :ok = :sys.resume(partition)
    end
  end

  # Waits until the room has handled earlier messages, such as a channel's DOWN.
  def sync_room(room) do
    if pid = room_pid(room) do
      try do
        :sys.get_state(pid)
      catch
        :exit, _stopped -> :ok
      end
    end

    :ok
  end

  def rejoin(room, player, peer) do
    UserSocket
    |> socket(peer, %{user: Accounts.get_user(player.user_id)})
    |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room}", %{
      "peer_id" => peer,
      "player_id" => player.id
    })
  end
end
