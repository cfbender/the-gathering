defmodule TheGatheringWeb.API.WebcamTableRoomControllerTest do
  use TheGatheringWeb.ConnCase, async: false
  import Phoenix.ChannelTest

  alias TheGathering.{AccountsFixtures, Games}
  alias TheGatheringWeb.{UserSocket, WebcamTableChannel}

  @endpoint TheGatheringWeb.Endpoint

  setup :register_and_log_in_user

  defp seat(room_id, peer_id, name) do
    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: name}, user.id)

    socket =
      UserSocket
      |> socket(peer_id, %{user: user})
      |> subscribe_and_join!(WebcamTableChannel, "webcam_table:#{room_id}", %{
        "peer_id" => peer_id,
        "player_id" => player.id
      })

    {socket, player}
  end

  test "lists live rooms with their seated players in join order", %{conn: conn} do
    assert %{"data" => []} = conn |> get(~p"/api/webcam-table/rooms") |> json_response(200)

    room_id = Ecto.UUID.generate()
    {_socket, alice} = seat(room_id, "peer-a", "Alice")
    {_socket, bob} = seat(room_id, "peer-b", "Bob")

    assert %{"data" => [room]} = conn |> get(~p"/api/webcam-table/rooms") |> json_response(200)
    assert room["id"] == room_id
    assert room["full"] == false
    assert is_integer(room["started_at"])

    assert room["players"] == [
             %{"id" => alice.id, "name" => "Alice"},
             %{"id" => bob.id, "name" => "Bob"}
           ]
  end

  test "drops a room once its last seat leaves", %{conn: conn} do
    room_id = Ecto.UUID.generate()
    {socket, _player} = seat(room_id, "peer-a", "Alice")
    @endpoint.subscribe("webcam_tables")

    assert %{"data" => [%{"id" => ^room_id}]} =
             conn |> get(~p"/api/webcam-table/rooms") |> json_response(200)

    Process.unlink(socket.channel_pid)
    close(socket)
    await_leave("peer-a")

    assert %{"data" => []} = conn |> get(~p"/api/webcam-table/rooms") |> json_response(200)
  end

  # The join diff may arrive after we subscribe, so skip diffs until the leave shows up.
  defp await_leave(peer_id) do
    assert_receive %Phoenix.Socket.Broadcast{event: "presence_diff", payload: %{leaves: leaves}}
    if not Map.has_key?(leaves, peer_id), do: await_leave(peer_id)
  end

  test "requires a signed-in user" do
    conn = build_conn() |> get(~p"/api/webcam-table/rooms")
    assert json_response(conn, 401)
  end
end
