import { useCallback, useEffect, useRef, useState } from "react"
import { parseDataMessage, type DataMessage } from "./data-messages"
import { canViewBoard, videoEncoding, type PublisherQuality } from "./media-policy"
import type { RoomLink } from "./room-link"
import type { TableParticipant } from "./room-types"
import type { LocalCamera } from "./use-local-camera"

export type Signal = { description: RTCSessionDescriptionInit } | { candidate: RTCIceCandidateInit }

/** Peer events other hooks can subscribe to without the peer layer knowing about them. */
export interface PeerListener {
  message?: (fromPeerId: string, message: DataMessage) => void
  /** The peer left the table (presence), not a reconnect or unmount. */
  left?: (peerId: string) => void
}

interface PeerState {
  connection: RTCPeerConnection
  videoSender: RTCRtpSender
  videoTrack: MediaStreamTrack
  mediaUpdate: Promise<void>
  /** Offer/answer/candidate steps run one at a time, in arrival order. */
  negotiation: Promise<void>
  channel?: RTCDataChannel
  candidates: RTCIceCandidateInit[]
  /** ICE was already restarted once after a failure; a second failure is reported, not retried. */
  restarted: boolean
}

/** What a remote seat's tile should say while there is no video from it yet. */
export function describeConnection(state: RTCPeerConnectionState | undefined): string {
  switch (state) {
    case "failed":
      return "Couldn't connect"
    case "disconnected":
      return "Reconnecting…"
    case "closed":
      return "Left"
    default:
      return "Connecting…"
  }
}

function withoutKey<T>(record: Record<string, T>, key: string) {
  const next = { ...record }
  delete next[key]
  return next
}

/** The WebRTC mesh: one connection per remote seat, its serialized signaling, the `table`
 * data channel, and which peers may receive this seat's video (spectators and private
 * reveals). Signals travel over the room channel in `link`. */
export function usePeerConnections(
  link: RoomLink,
  { stream, videoEnabled }: Pick<LocalCamera, "stream" | "videoEnabled">,
  quality: PublisherQuality,
  setError: (error: string) => void,
) {
  const peersRef = useRef(new Map<string, PeerState>())
  const listenersRef = useRef(new Set<PeerListener>())
  const qualityRef = useRef(quality)
  const iceServersRef = useRef<RTCIceServer[]>([])
  const [iceServers, setIceServersState] = useState<RTCIceServer[]>([])
  const [streams, setStreams] = useState<Record<string, MediaStream>>({})
  const [connectionStates, setConnectionStates] = useState<Record<string, RTCPeerConnectionState>>(
    {},
  )
  const revealToRef = useRef<string | null>(null)
  const [revealTo, setRevealTo] = useState<string | null>(null)
  const [revealBusy, setRevealBusy] = useState(false)

  const mayView = useCallback(
    (peerId: string) => !link.spectator && canViewBoard(link.peerId, peerId, revealToRef.current),
    [link],
  )

  // Disable private clones synchronously, then detach senders. Never disable the shared
  // native track to hide just one peer: the target and native crop RPC still need it.
  // This runs on every presence sync (each life tap at the table), so it only touches a
  // sender when something changed: in Chrome, `replaceTrack` with the track the sender
  // already has resets its encoder, and repeating that stalls every outgoing video.
  const syncVideo = useCallback(() => {
    const updates = [...peersRef.current].map(([id, peer]) => {
      peer.videoTrack.enabled = mayView(id) && videoEnabled()
      peer.mediaUpdate = peer.mediaUpdate
        .catch(() => {})
        .then(async () => {
          if (peer.connection.connectionState === "closed") return
          const track = mayView(id) ? peer.videoTrack : null
          if (peer.videoSender.track !== track) await peer.videoSender.replaceTrack(track)
          const parameters = peer.videoSender.getParameters()
          if (!parameters.encodings?.length) return
          const encoding = videoEncoding(
            link.participants.length,
            qualityRef.current,
            stream()?.getVideoTracks()[0]?.getSettings().height,
          )
          const unchanged = parameters.encodings.every(
            (current) =>
              current.scaleResolutionDownBy === encoding.scaleResolutionDownBy &&
              current.maxBitrate === encoding.maxBitrate,
          )
          if (unchanged) return
          parameters.encodings = parameters.encodings.map((current) => ({
            ...current,
            ...encoding,
          }))
          await peer.videoSender.setParameters(parameters)
        })
      return peer.mediaUpdate
    })
    return Promise.all(updates)
  }, [link, mayView, stream, videoEnabled])

  const refreshVideo = useCallback(() => {
    void syncVideo().catch(() =>
      setError("Could not update video senders. Hidden cameras remain blocked."),
    )
  }, [setError, syncVideo])

  useEffect(() => {
    qualityRef.current = quality
    refreshVideo()
  }, [quality, refreshVideo])

  /** Gives every peer a clone of the new camera track, then updates their senders. */
  const replaceSourceTrack = useCallback(
    (track: MediaStreamTrack) => {
      // Stop old clones before replacing, including clones detached by a private reveal.
      // syncVideo rechecks current consent inside each sender's serialized update.
      for (const [id, peer] of peersRef.current) {
        peer.videoTrack.stop()
        peer.videoTrack = track.clone()
        peer.videoTrack.enabled =
          track.enabled && canViewBoard(link.peerId, id, revealToRef.current)
      }
      return syncVideo()
    },
    [link, syncVideo],
  )

  const getPeerStats = useCallback(
    () =>
      Promise.all(
        [...peersRef.current].map(async ([id, peer]) => ({
          id,
          report: await peer.connection.getStats(),
        })),
      ),
    [],
  )

  const listen = useCallback((listener: PeerListener) => {
    listenersRef.current.add(listener)
    return () => {
      listenersRef.current.delete(listener)
    }
  }, [])

  /** Sends a message on the peer's data channel; false when it is not open or refuses it. */
  const send = useCallback((peerId: string, message: DataMessage) => {
    const channel = peersRef.current.get(peerId)?.channel
    if (channel?.readyState !== "open") return false
    try {
      channel.send(JSON.stringify(message))
      return true
    } catch {
      return false
    }
  }, [])

  const sendSignal = useCallback(
    (target: string, signal: Signal) => {
      link.channel?.push("signal", { target, signal })
    },
    [link],
  )

  /** Queues one signaling step behind the peer's previous ones. A step must check `current()`
   * after every await: the peer can leave or the channel can rejoin while it waits. Errors on
   * a live connection are logged; errors after it was replaced or closed are expected. */
  const negotiate = useCallback(
    (remotePeerId: string, peer: PeerState, step: (current: () => boolean) => Promise<void>) => {
      const current = () =>
        peersRef.current.get(remotePeerId) === peer && peer.connection.signalingState !== "closed"
      peer.negotiation = peer.negotiation.then(async () => {
        if (!current()) return
        try {
          await step(current)
        } catch (reason) {
          if (current()) console.warn(`WebRTC negotiation with ${remotePeerId} failed`, reason)
        }
      })
    },
    [],
  )

  const sendOffer = useCallback(
    (remotePeerId: string, peer: PeerState) =>
      negotiate(remotePeerId, peer, async (current) => {
        const description = await peer.connection.createOffer()
        if (!current()) return
        await peer.connection.setLocalDescription(description)
        if (current()) sendSignal(remotePeerId, { description })
      }),
    [negotiate, sendSignal],
  )

  const attachDataChannel = useCallback((peerId: string, dataChannel: RTCDataChannel) => {
    const peer = peersRef.current.get(peerId)
    if (!peer) return
    peer.channel = dataChannel
    // Data channels carry only the crop RPC; anything malformed or unexpected is dropped.
    dataChannel.onmessage = (event: MessageEvent<unknown>) => {
      const message = parseDataMessage(event.data)
      if (!message) return
      for (const listener of listenersRef.current) listener.message?.(peerId, message)
    }
  }, [])

  const createPeer = useCallback(
    (remotePeerId: string) => {
      const existing = peersRef.current.get(remotePeerId)
      if (existing) return existing
      const connection = new RTCPeerConnection({ iceServers: iceServersRef.current })
      const media = stream() as MediaStream
      const videoTrack = media.getVideoTracks()[0]!.clone()
      const visible = mayView(remotePeerId)
      videoTrack.enabled = visible && media.getVideoTracks()[0]!.enabled
      // addTrack lets an incoming offer reuse this transceiver on the answering side.
      const videoSender = connection.addTrack(videoTrack, media)
      const peer: PeerState = {
        connection,
        videoSender,
        videoTrack,
        mediaUpdate: visible ? Promise.resolve() : videoSender.replaceTrack(null),
        negotiation: Promise.resolve(),
        candidates: [],
        restarted: false,
      }
      peersRef.current.set(remotePeerId, peer)
      connection.onicecandidate = ({ candidate }) => {
        if (candidate) sendSignal(remotePeerId, { candidate: candidate.toJSON() })
      }
      connection.ontrack = ({ streams: incoming }) => {
        const remoteStream = incoming[0]
        if (remoteStream) setStreams((current) => ({ ...current, [remotePeerId]: remoteStream }))
      }
      connection.ondatachannel = ({ channel }) => attachDataChannel(remotePeerId, channel)
      connection.onconnectionstatechange = () => {
        const state = connection.connectionState
        if (state === "connected") refreshVideo()
        setConnectionStates((current) => ({ ...current, [remotePeerId]: state }))
        if (state === "failed" || state === "closed")
          setStreams((current) => withoutKey(current, remotePeerId))
        // One ICE restart covers a transient path loss; the side that made the first offer
        // makes the new one. If the networks simply cannot reach each other (no TURN), the
        // second failure stays on screen so the seat knows why.
        if (state === "failed" && !peer.restarted && link.peerId < remotePeerId) {
          peer.restarted = true
          connection.restartIce()
          sendOffer(remotePeerId, peer)
        }
      }
      return peer
    },
    [attachDataChannel, link, mayView, refreshVideo, sendOffer, sendSignal, stream],
  )

  const receiveSignal = useCallback(
    (remotePeerId: string, signal: Signal) => {
      const peer = createPeer(remotePeerId)
      negotiate(remotePeerId, peer, async (current) => {
        const { connection } = peer
        if ("candidate" in signal) {
          if (connection.remoteDescription) await connection.addIceCandidate(signal.candidate)
          else peer.candidates.push(signal.candidate)
          return
        }
        await connection.setRemoteDescription(signal.description)
        for (const candidate of peer.candidates.splice(0)) {
          if (!current()) return
          await connection.addIceCandidate(candidate)
        }
        if (signal.description.type !== "offer" || !current()) return
        const answer = await connection.createAnswer()
        if (!current()) return
        await connection.setLocalDescription(answer)
        if (current()) sendSignal(remotePeerId, { description: answer })
      })
    },
    [createPeer, negotiate, sendSignal],
  )

  /** Matches connections to everyone present: closes departed peers and connects newcomers.
   * The lower peer ID opens the data channel and makes the offer. */
  const syncPeers = useCallback(
    (everyone: TableParticipant[]) => {
      const activeIds = new Set(everyone.map((item) => item.peer_id))
      if (revealToRef.current && !activeIds.has(revealToRef.current)) {
        revealToRef.current = null
        setRevealTo(null)
      }
      for (const [id, peer] of peersRef.current) {
        if (activeIds.has(id)) continue
        peer.videoTrack.stop()
        peer.connection.close()
        peersRef.current.delete(id)
        setConnectionStates((current) => withoutKey(current, id))
        for (const listener of listenersRef.current) listener.left?.(id)
      }
      for (const participant of everyone) {
        const remoteId = participant.peer_id
        if (link.spectator && participant.spectator) continue
        if (remoteId === link.peerId || peersRef.current.has(remoteId)) continue
        const peer = createPeer(remoteId)
        if (link.peerId < remoteId) {
          attachDataChannel(remoteId, peer.connection.createDataChannel("table"))
          sendOffer(remoteId, peer)
        }
      }
      refreshVideo()
    },
    [attachDataChannel, createPeer, link, refreshVideo, sendOffer],
  )

  const closeAll = useCallback(() => {
    for (const peer of peersRef.current.values()) {
      peer.connection.onconnectionstatechange = null
      peer.videoTrack.stop()
      peer.connection.close()
    }
    peersRef.current.clear()
  }, [])

  /** A channel retry is a new media generation: drop every connection and its video. */
  const reset = useCallback(() => {
    closeAll()
    setStreams({})
    setConnectionStates({})
  }, [closeAll])

  const setIceServers = useCallback((servers: RTCIceServer[]) => {
    iceServersRef.current = servers
    setIceServersState(servers)
  }, [])

  const revealTarget = useCallback(() => revealToRef.current, [])

  /** Restores the reveal the server remembered for this seat after a (re)join. */
  const restoreReveal = useCallback((target: string | null) => {
    revealToRef.current = target
    setRevealTo(target)
  }, [])

  const changeReveal = useCallback(
    async (target: string | null) => {
      const channel = link.channel
      if (revealBusy || !channel) return
      setRevealBusy(true)
      revealToRef.current = target
      setRevealTo(target)
      try {
        await syncVideo()
        await new Promise<void>((resolve, reject) => {
          channel
            .push("reveal", { target })
            .receive("ok", () => resolve())
            .receive("error", () =>
              reject(
                new Error("Reveal target is no longer seated. End reveal to restore your camera."),
              ),
            )
            .receive("timeout", () =>
              reject(new Error("Reveal could not be confirmed. End reveal before trying again.")),
            )
        })
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : "Could not update reveal")
      } finally {
        setRevealBusy(false)
      }
    },
    [link, revealBusy, setError, syncVideo],
  )

  return {
    streams,
    connectionStates,
    iceServers,
    revealTo,
    revealBusy,
    refreshVideo,
    replaceSourceTrack,
    getPeerStats,
    listen,
    send,
    receiveSignal,
    syncPeers,
    reset,
    closeAll,
    setIceServers,
    revealTarget,
    restoreReveal,
    changeReveal,
  }
}

export type PeerConnections = ReturnType<typeof usePeerConnections>
