defmodule TheGathering.WebcamTables.Sfu.Subscription do
  @moduledoc """
  One viewer's copy of one publisher's video.

  The publisher sends up to three simulcast layers; the viewer receives exactly one. This
  module decides, packet by packet, which layer's packets are forwarded and rewrites their
  sequence numbers and timestamps so the viewer's decoder sees a single continuous stream
  across switches. A switch (or a resume after a private reveal) only happens on a keyframe
  of the new layer, because a decoder cannot pick up a stream mid-frame.

  It is pure: `TheGathering.WebcamTables.Sfu.Room` owns the processes and asks this module
  what to do with each packet. A publisher without simulcast has the single layer `:single`.
  """

  import Bitwise

  alias ExWebRTC.RTP.Munger
  alias ExWebRTC.RTPCodecParameters

  # How far behind the newest forwarded packet a late one may still arrive and be forwarded.
  # About a second of 1080p; anything older is as good as lost to the viewer anyway.
  @window 256
  @seen_mask (1 <<< @window) - 1
  @max_sn 0x10000

  @type layer :: String.t() | :single

  @type t :: %__MODULE__{
          owner_id: String.t(),
          transceiver_id: term(),
          sender_id: term(),
          track_id: term(),
          mid: String.t() | nil,
          wanted: layer(),
          layer: layer() | nil,
          pending: layer() | nil,
          allowed: boolean(),
          codec: RTPCodecParameters.t() | nil,
          munger: Munger.t() | nil,
          started: boolean(),
          newest: non_neg_integer() | nil,
          seen: non_neg_integer()
        }

  @enforce_keys [:owner_id, :transceiver_id, :sender_id, :track_id]
  defstruct @enforce_keys ++
              [
                mid: nil,
                wanted: "m",
                layer: nil,
                pending: nil,
                allowed: true,
                codec: nil,
                munger: nil,
                started: false,
                # The newest publisher sequence number forwarded from the current layer, and a
                # bit per earlier one (bit n = `newest - n`) saying whether it was forwarded.
                newest: nil,
                seen: 0
              ]

  @doc "A subscription that starts on `wanted` as soon as that layer sends a keyframe."
  def new(fields, wanted) do
    struct!(__MODULE__, fields) |> Map.merge(%{wanted: wanted, pending: wanted})
  end

  @doc """
  Records the codec the viewer's sender was switched to. Packets are rewritten in that
  codec's clock, and VP8 picture ids are made continuous as well.
  """
  def set_codec(%__MODULE__{} = sub, %RTPCodecParameters{} = codec) do
    %{sub | codec: codec, munger: Munger.new(codec)}
  end

  @doc "True once the sender codec is applied and the viewer may receive this board."
  def ready?(%__MODULE__{} = sub), do: sub.allowed and sub.munger != nil

  @doc """
  Asks for `layer`. Returns the subscription and whether a keyframe of that layer must be
  requested from the publisher before the switch can happen.
  """
  def request_layer(%__MODULE__{} = sub, layer) do
    cond do
      sub.layer == layer -> {%{sub | wanted: layer, pending: nil}, false}
      sub.pending == layer -> {%{sub | wanted: layer}, false}
      true -> {%{sub | wanted: layer, pending: layer}, true}
    end
  end

  @doc """
  Moves to `layer` without changing which one the viewer wants, for when the wanted layer
  has stopped arriving (or started again). Returns the subscription and whether a keyframe
  of `layer` must be requested.
  """
  def fall_back(%__MODULE__{} = sub, layer) do
    cond do
      sub.layer == layer -> {%{sub | pending: nil}, false}
      sub.pending == layer -> {sub, false}
      true -> {%{sub | pending: layer}, true}
    end
  end

  @doc """
  The layer in `live` a viewer wanting `wanted` should get: the sharpest live one no sharper
  than `wanted`, else the softest live one. `rids` lists layers softest first.
  """
  @spec nearest_live(layer(), [layer()], [layer()]) :: layer() | nil
  def nearest_live(wanted, live, rids) do
    wanted_at = Enum.find_index(rids, &(&1 == wanted)) || length(rids)

    rids
    |> Enum.with_index()
    |> Enum.filter(fn {rid, _index} -> rid in live end)
    |> Enum.min_by(fn {_rid, index} -> {index > wanted_at, abs(index - wanted_at)} end, fn ->
      nil
    end)
    |> case do
      {rid, _index} -> rid
      nil -> nil
    end
  end

  @doc """
  Allows or blocks forwarding. Blocking stops packets immediately; allowing again waits for
  a keyframe of the wanted layer, so the result also says whether to request one.
  """
  def set_allowed(%__MODULE__{allowed: allowed} = sub, allowed), do: {sub, false}

  def set_allowed(%__MODULE__{} = sub, false),
    do: {%{sub | allowed: false, layer: nil, pending: sub.wanted}, false}

  def set_allowed(%__MODULE__{} = sub, true),
    do: {%{sub | allowed: true, layer: nil, pending: sub.wanted}, true}

  @doc """
  Decides what to do with one packet the publisher sent on `rid`.

  Returns `{:forward, rewritten_packet, sub}` or `{:skip, sub}`. The pending layer is
  adopted on its first keyframe. A subscription that is not showing anything yet also
  adopts a keyframe of any other layer rather than staying black until the wanted layer
  delivers one (the browser may have paused that layer under CPU or bandwidth pressure).
  """
  def route(%__MODULE__{} = sub, rid, packet, keyframe?) do
    cond do
      not ready?(sub) ->
        {:skip, sub}

      rid == sub.layer ->
        forward(sub, packet)

      keyframe? and rid == sub.pending ->
        sub
        |> switch_to(rid)
        |> Map.put(:pending, nil)
        |> forward(packet)

      keyframe? and sub.layer == nil ->
        sub
        |> switch_to(rid)
        |> Map.put(:pending, if(sub.wanted == rid, do: nil, else: sub.wanted))
        |> forward(packet)

      true ->
        {:skip, sub}
    end
  end

  # The munger is told about a stream change only once something was forwarded; told before
  # its first packet, it would treat the second packet as the start of a new stream.
  defp switch_to(%{started: false} = sub, rid), do: %{sub | layer: rid, newest: nil, seen: 0}

  defp switch_to(sub, rid),
    do: %{sub | layer: rid, munger: Munger.update(sub.munger), newest: nil, seen: 0}

  defp forward(sub, packet) do
    case mark_seen(sub, packet.sequence_number) do
      {:new, sub} ->
        {packet, munger} = Munger.munge(sub.munger, packet)
        {:forward, packet, %{sub | munger: munger, started: true}}

      {:duplicate, sub} ->
        {:skip, sub}
    end
  end

  # Whether the publisher's packet is one this subscription has not forwarded yet. Browsers
  # resend recent packets over RTX to probe for bandwidth, and `ExWebRTC` hands those over
  # as the originals again; a sequence number sent twice fails the viewer's SRTP replay check.
  # Late packets within the window are still forwarded once.
  defp mark_seen(%{newest: nil} = sub, seq), do: {:new, %{sub | newest: seq, seen: 1}}

  defp mark_seen(%{newest: newest, seen: seen} = sub, seq) do
    ahead = rem(seq - newest + @max_sn, @max_sn)
    behind = @max_sn - ahead

    cond do
      ahead == 0 ->
        {:duplicate, sub}

      ahead < div(@max_sn, 2) ->
        seen = if ahead < @window, do: seen <<< ahead, else: 0
        {:new, %{sub | newest: seq, seen: (seen ||| 1) &&& @seen_mask}}

      behind >= @window or (seen >>> behind &&& 1) == 1 ->
        {:duplicate, sub}

      true ->
        {:new, %{sub | seen: seen ||| 1 <<< behind}}
    end
  end
end
