defmodule TheGathering.WebcamTables.Sfu.Room do
  @moduledoc """
  The SFU for one webcam table: a server-side `ExWebRTC.PeerConnection` per connected
  browser, and the forwarding of every publisher's chosen simulcast layer to every other
  connection.

  Signaling runs over the table's Phoenix channel. The browser makes the first offer (its
  camera as three layers); from then on only the server offers, adding or stopping a
  `sendonly` transceiver per other publisher, and the browser answers. Messages for a
  browser go to its channel process as `{:sfu, event, payload}`.

  Every peer connection is linked to this process, so a crashed connection surfaces here as
  an `:EXIT` and its channel is told to reconnect; a crashed room takes its connections
  down with it and every channel reconnects to a fresh one.
  """

  use GenServer, restart: :temporary

  require Logger

  alias ExRTCP.Packet.PayloadFeedback.PLI
  alias ExWebRTC.{ICECandidate, MediaStreamTrack, PeerConnection, SessionDescription}
  alias ExWebRTC.RTP.{H264, VP8}
  alias TheGathering.WebcamTables.Sfu
  alias TheGathering.WebcamTables.Sfu.{BrowserSdp, IceReport, SimulcastSdp, Subscription}

  # A keyframe request per publisher layer at most this often; a browser answering every
  # PLI from several viewers at once would spend its whole bitrate on keyframes.
  @pli_interval_ms 300

  # Browsers pause simulcast layers their uplink cannot carry and bring them back as the
  # estimate recovers. A layer silent this long counts as paused, and a viewer on it is
  # moved to one still arriving; it returns once the wanted layer has been back this long,
  # so a layer that keeps flapping does not drag the viewer back and forth.
  @adapt_interval_ms 500
  @stale_ms 500
  @recovered_ms 2_000

  # ex_ice gives up on a connection 8 s after it last heard from the browser on the selected
  # candidate pair, even while media is still arriving on another path (a NAT that rebinds, a
  # browser that moved to a pair it never nominated to us). An ICE restart keeps the DTLS
  # session, tracks and subscriptions and only redoes the path; the seat is dropped only when
  # restarts keep failing.
  @max_ice_restarts 3
  @ice_restart_window_ms 120_000

  def start_link(room_id), do: GenServer.start_link(__MODULE__, room_id, name: via(room_id))

  def via(room_id), do: {:via, Registry, {Sfu.Registry, room_id}}

  @impl true
  def init(room_id) do
    Process.flag(:trap_exit, true)
    Process.send_after(self(), :adapt_layers, @adapt_interval_ms)
    {:ok, %{id: room_id, peers: %{}, by_pc: %{}, by_monitor: %{}, plis: %{}}}
  end

  @impl true
  def handle_call({:join, peer_id, channel, spectator?}, _from, state) do
    state = if Map.has_key?(state.peers, peer_id), do: remove_peer(state, peer_id), else: state

    case PeerConnection.start_link(Sfu.peer_connection_options()) do
      {:ok, pc} ->
        peer = %{
          id: peer_id,
          channel: channel,
          monitor: Process.monitor(channel),
          spectator?: spectator?,
          pc: pc,
          reveal_to: nil,
          publisher: nil,
          subs: %{},
          sub_tracks: %{},
          ready?: false,
          negotiating?: false,
          dirty?: false,
          restart_ice?: false,
          ice_restarts: [],
          candidates: [],
          simulcast: %{}
        }

        state =
          state
          |> put_in([:peers, peer_id], peer)
          |> put_in([:by_pc, pc], peer_id)
          |> put_in([:by_monitor, peer.monitor], peer_id)

        {:reply, :ok, state}

      {:error, reason} ->
        Logger.error("SFU could not start a peer connection: #{inspect(reason)}")
        {:reply, {:error, :unavailable}, state}
    end
  end

  def handle_call({:offer, peer_id, sdp}, _from, state) do
    with {:ok, peer} <- fetch_peer(state, peer_id),
         :ok <- PeerConnection.set_remote_description(peer.pc, offer(sdp)),
         {:ok, answer} <- PeerConnection.create_answer(peer.pc),
         :ok <- PeerConnection.set_local_description(peer.pc, answer) do
      Enum.each(peer.candidates, &PeerConnection.add_ice_candidate(peer.pc, &1))
      peer = %{peer | ready?: true, candidates: [], simulcast: SimulcastSdp.receiving(sdp)}

      state =
        state
        |> put_peer(peer)
        |> subscribe_to_publishers(peer_id)
        |> negotiate(peer_id)

      {:reply, {:ok, answer.sdp}, state}
    else
      {:error, reason} = error ->
        Logger.warning("SFU rejected an offer from #{peer_id}: #{inspect(reason)}")
        {:reply, error, state}
    end
  end

  def handle_call({:answer, peer_id, sdp}, _from, state) do
    with {:ok, %{negotiating?: true} = peer} <- fetch_peer(state, peer_id),
         :ok <- PeerConnection.set_remote_description(peer.pc, answer(sdp)) do
      state =
        state
        |> put_peer(%{peer | negotiating?: false})
        |> apply_codecs(peer_id)
        |> negotiate(peer_id)

      {:reply, :ok, state}
    else
      {:ok, _peer} ->
        Logger.warning("SFU got an answer from #{peer_id} without an open offer")
        {:reply, {:error, :unexpected_answer}, state}

      {:error, reason} = error ->
        Logger.warning("SFU rejected an answer from #{peer_id}: #{inspect(reason)}")
        {:reply, error, state}
    end
  end

  def handle_call({:candidate, peer_id, json}, _from, state) do
    with {:ok, peer} <- fetch_peer(state, peer_id),
         {:ok, candidate} <- parse_candidate(json) do
      if peer.ready? do
        {:reply, PeerConnection.add_ice_candidate(peer.pc, candidate), state}
      else
        peer = %{peer | candidates: peer.candidates ++ [candidate]}
        {:reply, :ok, put_peer(state, peer)}
      end
    else
      error -> {:reply, error, state}
    end
  end

  def handle_call({:layer, peer_id, owner_id, layer}, _from, state) do
    with {:ok, peer} <- fetch_peer(state, peer_id),
         {:ok, %{publisher: %{rids: rids} = publisher}} when is_list(rids) <-
           fetch_peer(state, owner_id),
         %Subscription{} = sub <- Map.get(peer.subs, owner_id, {:error, :not_subscribed}) do
      layer = if layer in rids, do: layer, else: List.last(rids)
      {sub, keyframe?} = Subscription.request_layer(sub, layer)
      state = put_peer(state, put_in(peer.subs[owner_id], sub))
      state = if keyframe?, do: request_keyframe(state, owner_id, publisher, layer), else: state
      {:reply, :ok, state}
    else
      # A publisher without simulcast has nothing to choose from.
      {:ok, _peer} -> {:reply, :ok, state}
      {:error, _reason} = error -> {:reply, error, state}
    end
  end

  def handle_call({:reveal, peer_id, target}, _from, state) do
    case fetch_peer(state, peer_id) do
      {:ok, peer} ->
        peer = %{peer | reveal_to: target}
        {:reply, :ok, state |> put_peer(peer) |> refresh_allowed(peer)}

      error ->
        {:reply, error, state}
    end
  end

  def handle_call({:relay, from, to, message}, _from, state) do
    case fetch_peer(state, to) do
      {:ok, peer} ->
        send(peer.channel, {:sfu, :peer_message, %{from: from, message: message}})
        {:reply, :ok, state}

      error ->
        {:reply, error, state}
    end
  end

  @impl true
  def handle_info({:ex_webrtc, pc, message}, state) do
    case Map.fetch(state.by_pc, pc) do
      {:ok, peer_id} -> {:noreply, handle_webrtc(message, peer_id, state)}
      :error -> {:noreply, state}
    end
  end

  def handle_info(:adapt_layers, state) do
    Process.send_after(self(), :adapt_layers, @adapt_interval_ms)
    {:noreply, adapt_layers(state, System.monotonic_time(:millisecond))}
  end

  # The channel left or crashed; its connection and everyone's copy of its video go too.
  def handle_info({:DOWN, ref, :process, _pid, _reason}, state) do
    case Map.pop(state.by_monitor, ref) do
      {nil, _by_monitor} -> {:noreply, state}
      {peer_id, _by_monitor} -> state |> remove_peer(peer_id) |> stop_when_empty()
    end
  end

  # A peer connection crashed. Its browser reconnects through the channel with a new seat
  # connection, which brings a new peer id and a fresh connection here.
  def handle_info({:EXIT, pc, reason}, state) do
    case Map.fetch(state.by_pc, pc) do
      {:ok, peer_id} ->
        Logger.warning("SFU peer connection for #{peer_id} exited: #{inspect(reason)}")
        state |> disconnect_peer(peer_id, reason) |> stop_when_empty()

      :error ->
        {:noreply, state}
    end
  end

  @impl true
  def terminate(_reason, state) do
    Enum.each(state.peers, fn {_id, peer} -> PeerConnection.stop(peer.pc) end)
  end

  # --- WebRTC events ------------------------------------------------------------------------

  defp handle_webrtc({:track, track}, peer_id, state) do
    peer = Map.fetch!(state.peers, peer_id)

    if peer.spectator? or peer.publisher != nil or track.kind != :video do
      state
    else
      codecs =
        peer.pc
        |> PeerConnection.get_transceivers()
        |> Enum.find_value([], fn tr -> tr.receiver.track.id == track.id && tr.codecs end)

      publisher = %{track_id: track.id, rids: track.rids, codec: nil, codecs: codecs, layers: %{}}
      state = put_peer(state, %{peer | publisher: publisher})

      state.peers
      |> Map.keys()
      |> Enum.reject(&(&1 == peer_id))
      |> Enum.reduce(state, fn viewer_id, state ->
        state |> subscribe(viewer_id, peer_id) |> negotiate(viewer_id)
      end)
    end
  end

  defp handle_webrtc({:rtp, track_id, rid, packet}, peer_id, state) do
    case Map.fetch!(state.peers, peer_id) do
      %{publisher: %{track_id: ^track_id} = publisher} = peer ->
        rid = rid || :single
        publisher = note_layer(publisher, rid, System.monotonic_time(:millisecond))

        state =
          state
          |> put_peer(%{peer | publisher: publisher})
          |> ensure_publisher_codec(peer_id, publisher, packet)

        case state.peers[peer_id].publisher do
          %{codec: nil} -> state
          publisher -> forward(state, peer_id, publisher, rid, packet)
        end

      _peer ->
        state
    end
  end

  # A viewer's decoder lost sync and asks for a keyframe; the publisher's layer it is
  # watching is the one that must send it.
  defp handle_webrtc({:rtcp, packets}, peer_id, state) do
    peer = Map.fetch!(state.peers, peer_id)

    Enum.reduce(packets, state, fn
      {track_id, %PLI{}}, state ->
        with owner_id when is_binary(owner_id) <- Map.get(peer.sub_tracks, track_id),
             %Subscription{} = sub <- Map.get(peer.subs, owner_id),
             {:ok, %{publisher: %{} = publisher}} <- fetch_peer(state, owner_id) do
          request_keyframe(state, owner_id, publisher, sub.layer || sub.pending)
        else
          _other -> state
        end

      _packet, state ->
        state
    end)
  end

  defp handle_webrtc({:ice_candidate, candidate}, peer_id, state) do
    peer = Map.fetch!(state.peers, peer_id)
    send(peer.channel, {:sfu, :candidate, %{candidate: ICECandidate.to_json(candidate)}})
    state
  end

  defp handle_webrtc({:connection_state_change, :failed}, peer_id, state) do
    peer = Map.fetch!(state.peers, peer_id)
    now = System.monotonic_time(:millisecond)
    recent = Enum.filter(peer.ice_restarts, &(now - &1 < @ice_restart_window_ms))
    Logger.info("SFU peer connection for #{peer_id} failed; ICE: #{IceReport.describe(peer.pc)}")

    if length(recent) < @max_ice_restarts and peer.ready? do
      Logger.info(
        "SFU restarting ICE for #{peer_id} (#{length(recent) + 1}/#{@max_ice_restarts})"
      )

      state
      |> put_peer(%{peer | restart_ice?: true, dirty?: true, ice_restarts: [now | recent]})
      |> negotiate(peer_id)
    else
      disconnect_peer(state, peer_id, :failed)
    end
  end

  defp handle_webrtc(_message, _peer_id, state), do: state

  # When the layer last sent a packet, and since when it has been sending without a pause.
  defp note_layer(publisher, rid, now) do
    since =
      case publisher.layers[rid] do
        %{last: last, since: since} when now - last < @stale_ms -> since
        _paused_or_new -> now
      end

    put_in(publisher.layers[rid], %{last: now, since: since})
  end

  # --- Subscriptions ------------------------------------------------------------------------

  defp subscribe_to_publishers(state, viewer_id) do
    state.peers
    |> Enum.filter(fn {id, peer} -> id != viewer_id and peer.publisher != nil end)
    |> Enum.reduce(state, fn {owner_id, _peer}, state -> subscribe(state, viewer_id, owner_id) end)
  end

  # Gives `viewer_id` a sendonly transceiver that will carry `owner_id`'s video. The stream
  # id is the owner's peer id, so the browser can tell whose board arrived.
  defp subscribe(state, viewer_id, owner_id) do
    viewer = Map.fetch!(state.peers, viewer_id)
    owner = Map.fetch!(state.peers, owner_id)

    if Map.has_key?(viewer.subs, owner_id) or is_nil(owner.publisher) do
      state
    else
      track = MediaStreamTrack.new(:video, [owner_id])
      {:ok, tr} = PeerConnection.add_transceiver(viewer.pc, track, direction: :sendonly)
      wanted = if owner.publisher.rids, do: "m", else: :single

      {sub, _keyframe?} =
        %{owner_id: owner_id, transceiver_id: tr.id, sender_id: tr.sender.id, track_id: track.id}
        |> Subscription.new(wanted)
        |> Subscription.set_allowed(allowed?(owner, viewer))

      viewer = %{
        viewer
        | subs: Map.put(viewer.subs, owner_id, sub),
          sub_tracks: Map.put(viewer.sub_tracks, track.id, owner_id),
          dirty?: true
      }

      put_peer(state, viewer)
    end
  end

  defp unsubscribe(state, viewer_id, owner_id) do
    viewer = Map.fetch!(state.peers, viewer_id)

    case Map.pop(viewer.subs, owner_id) do
      {nil, _subs} ->
        state

      {sub, subs} ->
        _ = PeerConnection.stop_transceiver(viewer.pc, sub.transceiver_id)

        viewer = %{
          viewer
          | subs: subs,
            sub_tracks: Map.delete(viewer.sub_tracks, sub.track_id),
            dirty?: true
        }

        state |> put_peer(viewer) |> negotiate(viewer_id)
    end
  end

  # Only the owner's chosen viewer sees a private reveal.
  defp allowed?(%{reveal_to: nil}, _viewer), do: true
  defp allowed?(%{reveal_to: viewer_id}, %{id: viewer_id}), do: true
  defp allowed?(_owner, _viewer), do: false

  defp refresh_allowed(state, %{publisher: nil}), do: state

  defp refresh_allowed(state, owner) do
    Enum.reduce(state.peers, state, fn {_viewer_id, viewer}, state ->
      refresh_allowed(state, owner, viewer, Map.get(viewer.subs, owner.id))
    end)
  end

  defp refresh_allowed(state, _owner, _viewer, nil), do: state

  defp refresh_allowed(state, owner, viewer, sub) do
    {sub, keyframe?} = Subscription.set_allowed(sub, allowed?(owner, viewer))
    state = put_peer(state, put_in(viewer.subs[owner.id], sub))

    if keyframe?,
      do: request_keyframe(state, owner.id, owner.publisher, sub.pending),
      else: state
  end

  # --- Layer liveness -----------------------------------------------------------------------

  # Moves viewers off publisher layers that have gone quiet and back once the wanted one
  # has reliably returned. Only simulcast subscriptions that are already showing a layer
  # take part; a blank one adopts whatever keyframe arrives first (see `Subscription.route/4`).
  defp adapt_layers(state, now) do
    Enum.reduce(state.peers, state, fn {viewer_id, viewer}, state ->
      Enum.reduce(viewer.subs, state, fn
        {owner_id, %{layer: layer} = sub}, state when is_binary(layer) ->
          adapt_layer(state, viewer_id, owner_id, sub, now)

        _blank_or_single, state ->
          state
      end)
    end)
  end

  defp adapt_layer(state, viewer_id, owner_id, sub, now) do
    %{publisher: %{rids: rids} = publisher} = Map.fetch!(state.peers, owner_id)
    live = for {rid, %{last: last}} <- publisher.layers, now - last < @stale_ms, do: rid

    target =
      cond do
        sub.layer not in live -> Subscription.nearest_live(sub.wanted, live, rids)
        sub.layer != sub.wanted and recovered?(publisher, sub.wanted, now) -> sub.wanted
        true -> nil
      end

    with rid when is_binary(rid) <- target,
         {sub, true} <- Subscription.fall_back(sub, rid) do
      Logger.info(
        "SFU moving #{viewer_id} from #{owner_id}'s #{sub.layer} layer to #{rid} " <>
          "(wants #{sub.wanted}; live: #{Enum.join(live, ",")})"
      )

      viewer = Map.fetch!(state.peers, viewer_id)

      state
      |> put_peer(put_in(viewer.subs[owner_id], sub))
      |> request_keyframe(owner_id, publisher, rid)
    else
      _nothing_to_do -> state
    end
  end

  defp recovered?(publisher, rid, now) do
    case publisher.layers[rid] do
      %{last: last, since: since} -> now - last < @stale_ms and now - since >= @recovered_ms
      nil -> false
    end
  end

  # --- Negotiation --------------------------------------------------------------------------

  # Offers the viewer's current set of transceivers, one negotiation at a time. Changes
  # made while an offer is outstanding wait for its answer; the browser never offers again.
  defp negotiate(state, peer_id) do
    case Map.fetch!(state.peers, peer_id) do
      %{ready?: true, negotiating?: false, dirty?: true} = peer -> offer(state, peer)
      _peer -> state
    end
  end

  defp offer(state, peer) do
    with {:ok, offer} <- PeerConnection.create_offer(peer.pc, ice_restart: peer.restart_ice?),
         :ok <- PeerConnection.set_local_description(peer.pc, offer) do
      mids =
        peer.pc
        |> PeerConnection.get_transceivers()
        |> Map.new(&{&1.id, &1.mid})

      subs =
        Map.new(peer.subs, fn {owner_id, sub} ->
          {owner_id, %{sub | mid: mids[sub.transceiver_id]}}
        end)

      tracks = Map.new(subs, fn {owner_id, sub} -> {sub.mid, owner_id} end)
      sdp = SimulcastSdp.restore(offer.sdp, peer.simulcast)
      restart = if peer.restart_ice?, do: " with an ICE restart", else: ""
      Logger.info("SFU offered #{map_size(tracks)} board(s) to #{peer.id}#{restart}")
      send(peer.channel, {:sfu, :offer, %{sdp: sdp, tracks: tracks}})

      put_peer(state, %{peer | subs: subs, negotiating?: true, dirty?: false, restart_ice?: false})
    else
      {:error, reason} ->
        Logger.warning("SFU could not offer to #{peer.id}: #{inspect(reason)}")
        state
    end
  end

  # Once the browser has answered, each new subscription's sender is switched to the codec
  # its publisher actually sends; `send_rtp` then stamps the right payload type on every
  # forwarded packet. A browser that did not negotiate that codec gets nothing for that board.
  defp apply_codecs(state, viewer_id) do
    viewer = Map.fetch!(state.peers, viewer_id)
    transceivers = PeerConnection.get_transceivers(viewer.pc)

    Enum.reduce(viewer.subs, state, fn
      {_owner_id, %{codec: codec}}, state when codec != nil ->
        state

      {owner_id, sub}, state ->
        with {:ok, %{publisher: %{codec: %{} = codec} = publisher}} <- fetch_peer(state, owner_id),
             %{current_direction: direction} = tr when direction != nil <-
               Enum.find(transceivers, &(&1.id == sub.transceiver_id)),
             %{} = match <- matching_codec(tr.codecs, codec),
             :ok <- PeerConnection.set_sender_codec(viewer.pc, sub.sender_id, match) do
          sub = Subscription.set_codec(sub, match)
          viewer = Map.fetch!(state.peers, viewer_id)
          state = put_peer(state, put_in(viewer.subs[owner_id], sub))
          request_keyframe(state, owner_id, publisher, sub.pending)
        else
          nil ->
            Logger.warning(
              "SFU viewer #{viewer_id} did not negotiate #{codec_name(state, owner_id)}"
            )

            state

          _other ->
            state
        end
    end)
  end

  # The viewer's negotiated entry for the publisher's codec, under whatever payload type that
  # viewer assigned it. Only the format parameters that change the bitstream count: H.264
  # profile and packetization mode. Browsers decorate the rest differently (Firefox's VP8
  # carries `max-fs`/`max-fr`, Chrome's nothing), and a viewer decodes either just the same.
  defp matching_codec(codecs, codec) do
    Enum.find(codecs, fn candidate ->
      String.downcase(candidate.mime_type) == String.downcase(codec.mime_type) and
        candidate.clock_rate == codec.clock_rate and
        bitstream_params(candidate) == bitstream_params(codec)
    end)
  end

  defp bitstream_params(%{mime_type: mime, sdp_fmtp_line: fmtp}) do
    if String.downcase(mime) == "video/h264" do
      {fmtp && fmtp.profile_level_id, (fmtp && fmtp.packetization_mode) || 0}
    end
  end

  defp codec_name(state, owner_id) do
    case state.peers[owner_id] do
      %{publisher: %{codec: %{mime_type: mime}}} -> mime
      _other -> "the publisher's codec"
    end
  end

  # --- Forwarding ---------------------------------------------------------------------------

  # The publisher's codec is whatever payload type its first packet carries.
  defp ensure_publisher_codec(state, _peer_id, %{codec: %{}}, _packet), do: state

  defp ensure_publisher_codec(state, peer_id, publisher, packet) do
    case Enum.find(publisher.codecs, &(&1.payload_type == packet.payload_type)) do
      %{mime_type: mime} = codec when mime in ["video/H264", "video/VP8"] ->
        peer = Map.fetch!(state.peers, peer_id)
        state = put_peer(state, %{peer | publisher: %{publisher | codec: codec}})

        state.peers
        |> Map.keys()
        |> Enum.reject(&(&1 == peer_id))
        |> Enum.reduce(state, &apply_codecs(&2, &1))

      _unknown ->
        state
    end
  end

  defp forward(state, owner_id, publisher, rid, packet) do
    keyframe? = keyframe?(publisher.codec, packet)

    Enum.reduce(state.peers, state, fn {_viewer_id, viewer}, state ->
      case Map.get(viewer.subs, owner_id) do
        nil ->
          state

        sub ->
          deliver(state, viewer, Subscription.route(sub, rid, packet, keyframe?), publisher, rid)
      end
    end)
  end

  defp deliver(state, viewer, {:forward, packet, sub}, _publisher, _rid) do
    PeerConnection.send_rtp(viewer.pc, sub.track_id, packet)
    put_peer(state, put_in(viewer.subs[sub.owner_id], sub))
  end

  defp deliver(state, viewer, {:skip, sub}, publisher, rid) do
    state = put_peer(state, put_in(viewer.subs[sub.owner_id], sub))

    # The layer this viewer is waiting for is live but has not sent a keyframe.
    if Subscription.ready?(sub) and sub.pending == rid,
      do: request_keyframe(state, sub.owner_id, publisher, rid),
      else: state
  end

  defp keyframe?(%{mime_type: "video/H264"}, packet), do: H264.keyframe?(packet)
  defp keyframe?(%{mime_type: "video/VP8"}, packet), do: VP8.keyframe?(packet)

  defp request_keyframe(state, _owner_id, _publisher, nil), do: state

  defp request_keyframe(state, owner_id, publisher, layer) do
    # Monotonic time may be negative, so "never asked" is a missing key rather than a sentinel.
    now = System.monotonic_time(:millisecond)
    key = {owner_id, layer}

    case Map.fetch(state.plis, key) do
      {:ok, last} when now - last < @pli_interval_ms ->
        state

      _never_or_long_ago ->
        pc = Map.fetch!(state.peers, owner_id).pc
        rid = if layer == :single, do: nil, else: layer
        PeerConnection.send_pli(pc, publisher.track_id, rid)
        put_in(state.plis[key], now)
    end
  end

  # --- Peers --------------------------------------------------------------------------------

  defp disconnect_peer(state, peer_id, reason) do
    case fetch_peer(state, peer_id) do
      {:ok, peer} ->
        send(peer.channel, {:sfu, :down, reason})
        remove_peer(state, peer_id)

      _error ->
        state
    end
  end

  defp remove_peer(state, peer_id) do
    case Map.pop(state.peers, peer_id) do
      {nil, _peers} ->
        state

      {peer, peers} ->
        Process.demonitor(peer.monitor, [:flush])
        if Process.alive?(peer.pc), do: PeerConnection.stop(peer.pc)

        state = %{
          state
          | peers: peers,
            by_pc: Map.delete(state.by_pc, peer.pc),
            by_monitor: Map.delete(state.by_monitor, peer.monitor),
            plis: Map.reject(state.plis, fn {{owner_id, _layer}, _at} -> owner_id == peer_id end)
        }

        Enum.reduce(Map.keys(peers), state, &unsubscribe(&2, &1, peer_id))
    end
  end

  defp stop_when_empty(%{peers: peers} = state) when map_size(peers) == 0,
    do: {:stop, :normal, state}

  defp stop_when_empty(state), do: {:noreply, state}

  defp fetch_peer(state, peer_id) do
    case Map.fetch(state.peers, peer_id) do
      {:ok, peer} -> {:ok, peer}
      :error -> {:error, :not_joined}
    end
  end

  defp put_peer(state, peer), do: put_in(state.peers[peer.id], peer)

  defp offer(sdp), do: %SessionDescription{type: :offer, sdp: BrowserSdp.unify_dtls_roles(sdp)}

  defp answer(sdp),
    do: %SessionDescription{type: :answer, sdp: BrowserSdp.unify_dtls_roles(sdp)}

  defp parse_candidate(%{"candidate" => candidate} = json) when is_binary(candidate) do
    {:ok,
     %ICECandidate{
       candidate: candidate,
       sdp_mid: json["sdpMid"],
       sdp_m_line_index: json["sdpMLineIndex"],
       username_fragment: json["usernameFragment"]
     }}
  end

  defp parse_candidate(_json), do: {:error, :invalid_candidate}
end
