defmodule TheGathering.WebcamTables.Sfu.SubscriptionTest do
  use ExUnit.Case, async: true

  alias ExRTP.Packet
  alias ExWebRTC.RTPCodecParameters
  alias TheGathering.WebcamTables.Sfu.Subscription

  # H.264 packets need no payload rewriting, so any payload serves as a frame here.
  @codec %RTPCodecParameters{payload_type: 96, mime_type: "video/H264", clock_rate: 90_000}

  defp subscription(wanted) do
    %{owner_id: "owner", transceiver_id: 1, sender_id: 2, track_id: 3}
    |> Subscription.new(wanted)
    |> Subscription.set_codec(@codec)
  end

  defp packet(seq, timestamp \\ 0),
    do: Packet.new(<<seq>>, payload_type: 96, sequence_number: seq, timestamp: timestamp)

  test "nothing is forwarded until the sender codec is applied" do
    sub = Subscription.new(%{owner_id: "o", transceiver_id: 1, sender_id: 2, track_id: 3}, "m")
    refute Subscription.ready?(sub)
    assert {:skip, ^sub} = Subscription.route(sub, "m", packet(1), true)
  end

  test "starts on the wanted layer at its first keyframe, dropping earlier delta frames" do
    sub = subscription("m")
    assert {:skip, sub} = Subscription.route(sub, "m", packet(1), false)
    assert {:skip, sub} = Subscription.route(sub, "h", packet(1), false)
    assert {:forward, _packet, sub} = Subscription.route(sub, "m", packet(2), true)
    assert sub.layer == "m" and sub.pending == nil
    assert {:skip, _sub} = Subscription.route(sub, "h", packet(3), false)
  end

  test "switches layers only on a keyframe of the new layer and keeps sequence numbers continuous" do
    sub = subscription("l")
    {:forward, first, sub} = Subscription.route(sub, "l", packet(100, 1_000), true)
    {:forward, second, sub} = Subscription.route(sub, "l", packet(101, 1_000), false)
    assert second.sequence_number == first.sequence_number + 1

    assert {sub, true} = Subscription.request_layer(sub, "h")
    assert sub.layer == "l" and sub.pending == "h"

    # Delta frames on the new layer are dropped; the old layer keeps flowing meanwhile.
    assert {:skip, sub} = Subscription.route(sub, "h", packet(5_000, 9_000), false)
    assert {:forward, third, sub} = Subscription.route(sub, "l", packet(102, 1_000), false)
    assert third.sequence_number == second.sequence_number + 1

    # The high layer's sequence numbers are far away, but the viewer sees the next number.
    assert {:forward, fourth, sub} = Subscription.route(sub, "h", packet(5_001, 9_000), true)
    assert fourth.sequence_number == third.sequence_number + 1
    assert sub.layer == "h" and sub.pending == nil
    assert {:skip, _sub} = Subscription.route(sub, "l", packet(103, 1_000), true)
  end

  test "asking for the current or already pending layer needs no new keyframe" do
    sub = subscription("m")
    {:forward, _packet, sub} = Subscription.route(sub, "m", packet(1), true)
    assert {_sub, false} = Subscription.request_layer(sub, "m")

    {sub, true} = Subscription.request_layer(sub, "h")
    assert {_sub, false} = Subscription.request_layer(sub, "h")
  end

  test "a hidden board stops immediately and resumes only on a keyframe" do
    sub = subscription("m")
    {:forward, _packet, sub} = Subscription.route(sub, "m", packet(1), true)

    assert {sub, false} = Subscription.set_allowed(sub, false)
    refute Subscription.ready?(sub)
    assert {:skip, sub} = Subscription.route(sub, "m", packet(2), true)

    assert {sub, true} = Subscription.set_allowed(sub, true)
    assert sub.pending == "m"
    assert {:skip, sub} = Subscription.route(sub, "m", packet(3), false)
    assert {:forward, _packet, sub} = Subscription.route(sub, "m", packet(4), true)
    assert {_sub, false} = Subscription.set_allowed(sub, true)
  end

  test "a blank viewer adopts any layer's keyframe but keeps waiting for the wanted one" do
    sub = subscription("h")
    assert {:forward, _packet, sub} = Subscription.route(sub, "l", packet(1), true)
    assert sub.layer == "l" and sub.pending == "h"
    assert {:forward, _packet, sub} = Subscription.route(sub, "l", packet(2), false)
    assert {:forward, _packet, sub} = Subscription.route(sub, "h", packet(50), true)
    assert sub.layer == "h" and sub.pending == nil
  end

  test "a publisher without simulcast has one layer" do
    sub = subscription(:single)
    assert {:forward, _packet, sub} = Subscription.route(sub, :single, packet(1), true)
    assert {:forward, _packet, _sub} = Subscription.route(sub, :single, packet(2), false)
  end

  test "a packet the publisher resends is forwarded once, a late one still once" do
    sub = subscription("m")
    {:forward, _packet, sub} = Subscription.route(sub, "m", packet(10), true)
    {:forward, _packet, sub} = Subscription.route(sub, "m", packet(11), false)
    # Packet 12 is lost for now; 13 arrives.
    {:forward, _packet, sub} = Subscription.route(sub, "m", packet(13), false)

    # RTX probing resends the newest and an older packet.
    assert {:skip, sub} = Subscription.route(sub, "m", packet(13), false)
    assert {:skip, sub} = Subscription.route(sub, "m", packet(11), false)

    # The genuinely late packet gets through, but only the first time.
    assert {:forward, late, sub} = Subscription.route(sub, "m", packet(12), false)
    assert late.sequence_number == 12
    assert {:skip, sub} = Subscription.route(sub, "m", packet(12), false)

    assert {:forward, _packet, _sub} = Subscription.route(sub, "m", packet(14), false)
  end

  test "duplicate detection survives sequence number wraparound" do
    sub = subscription("m")
    {:forward, _packet, sub} = Subscription.route(sub, "m", packet(65_534), true)
    {:forward, _packet, sub} = Subscription.route(sub, "m", packet(65_535), false)
    {:forward, _packet, sub} = Subscription.route(sub, "m", packet(1), false)

    assert {:skip, sub} = Subscription.route(sub, "m", packet(65_535), false)
    assert {:forward, _packet, sub} = Subscription.route(sub, "m", packet(0), false)
    assert {:skip, sub} = Subscription.route(sub, "m", packet(0), false)
    # Far too old to be a late packet; it would fail the viewer's replay check anyway.
    assert {:skip, _sub} = Subscription.route(sub, "m", packet(60_000), false)
  end

  test "the duplicate window starts over on a layer switch" do
    sub = subscription("l")
    {:forward, _packet, sub} = Subscription.route(sub, "l", packet(500), true)
    {:forward, _packet, sub} = Subscription.route(sub, "l", packet(501), false)

    {sub, true} = Subscription.request_layer(sub, "h")
    # The new layer happens to reuse numbers the old one already forwarded.
    assert {:forward, _packet, sub} = Subscription.route(sub, "h", packet(500), true)
    assert {:forward, _packet, sub} = Subscription.route(sub, "h", packet(501), false)
    assert {:skip, _sub} = Subscription.route(sub, "h", packet(501), false)
  end
end
