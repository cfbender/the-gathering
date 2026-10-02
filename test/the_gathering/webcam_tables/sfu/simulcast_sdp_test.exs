defmodule TheGathering.WebcamTables.Sfu.SimulcastSdpTest do
  use ExUnit.Case, async: true

  alias TheGathering.WebcamTables.Sfu.SimulcastSdp

  @session """
  v=0
  o=- 1 2 IN IP4 127.0.0.1
  s=-
  t=0 0
  a=group:BUNDLE 0 1
  """

  defp mline(mid, attrs) do
    """
    m=video 9 UDP/TLS/RTP/SAVPF 96
    c=IN IP4 0.0.0.0
    a=mid:#{mid}
    a=rtpmap:96 H264/90000
    """ <> Enum.map_join(attrs, "", &"#{&1}\n")
  end

  defp sdp(parts), do: String.replace(@session <> Enum.join(parts), "\n", "\r\n")

  defp simulcast_offer do
    sdp([
      mline("0", [
        "a=sendonly",
        "a=rid:l send",
        "a=rid:m send",
        "a=rid:h send",
        "a=simulcast:send l;m;h"
      ]),
      mline("1", ["a=recvonly"])
    ])
  end

  test "receiving/1 reverses the browser's send layers for the sections that have them" do
    assert %{"0" => attrs} = received = SimulcastSdp.receiving(simulcast_offer())
    refute Map.has_key?(received, "1")

    assert Enum.map(attrs, &to_string/1) == [
             "rid:l recv",
             "rid:m recv",
             "rid:h recv",
             "simulcast:recv l;m;h"
           ]
  end

  test "receiving/1 is empty for a spectator offer and for garbage" do
    assert SimulcastSdp.receiving(sdp([mline("0", ["a=recvonly"])])) == %{}
    assert SimulcastSdp.receiving("not sdp") == %{}
  end

  test "restore/2 adds the layers to the matching section only" do
    attrs = SimulcastSdp.receiving(simulcast_offer())

    server_offer =
      sdp([
        mline("0", ["a=recvonly"]),
        mline("1", ["a=sendonly", "a=msid:owner-a track-a"]),
        mline("2", ["a=sendonly", "a=msid:owner-b track-b"])
      ])

    assert {:ok, %ExSDP{media: [camera, board_a, board_b]}} =
             server_offer |> SimulcastSdp.restore(attrs) |> ExSDP.parse()

    assert ExSDP.get_attribute(camera, :simulcast) ==
             %ExSDP.Attribute.Simulcast{recv: ["l", "m", "h"], send: []}

    assert camera |> ExSDP.get_attributes(:rid) |> Enum.map(& &1.id) == ["l", "m", "h"]
    assert Enum.all?(ExSDP.get_attributes(camera, :rid), &(&1.direction == :recv))

    for board <- [board_a, board_b] do
      assert ExSDP.get_attribute(board, :simulcast) == nil
      assert ExSDP.get_attributes(board, :rid) == []
    end
  end

  test "restore/2 leaves the offer alone when there is nothing to restore" do
    offer = sdp([mline("0", ["a=recvonly"])])
    assert SimulcastSdp.restore(offer, %{}) == offer
  end
end
