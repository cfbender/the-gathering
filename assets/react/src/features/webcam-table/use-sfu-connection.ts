import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { parseDataMessage, type DataMessage } from "./data-messages"
import { orderVideoCodecs, videoEncoding, type PublisherQuality } from "./media-policy"
import type { RoomLink } from "./room-link"
import type { TableParticipant } from "./room-types"
import type { LocalCamera } from "./use-local-camera"

/** Simulcast layers the camera publishes, lowest resolution first, as the server names them. */
export type SimulcastLayer = "l" | "m" | "h"
export const SIMULCAST_LAYERS: readonly SimulcastLayer[] = ["l", "m", "h"]

/** A server offer: the SDP plus whose board each `sendonly` m-line (by mid) carries. */
export interface SfuOffer {
  sdp: string
  tracks: Record<string, string>
}

/** Peer events other hooks can subscribe to without the connection layer knowing about them. */
export interface PeerListener {
  message?: (fromPeerId: string, message: DataMessage) => void
  /** The peer left the table (presence), not a reconnect or unmount. */
  left?: (peerId: string) => void
}

interface Connection {
  pc: RTCPeerConnection
  /** This seat's camera sender; spectators publish nothing. */
  sender: RTCRtpSender | null
  /** A private clone of the camera track, so the native track stays untouched for crops. */
  videoTrack: MediaStreamTrack | null
  /** Offer/answer/candidate steps run one at a time, in arrival order. */
  negotiation: Promise<void>
  /** Server candidates that arrived before its description. */
  candidates: RTCIceCandidateInit[]
  /** mid → owner peer id for every board the server sends on this connection. */
  tracks: Record<string, string>
}

interface Tile {
  streamId: string
  height: number
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

/** The browser's candidate pairs from a `getStats()` report, one line each: which of our
 * addresses talked to which of the server's, whether it was the pair in use, and how much
 * travelled on it. Logged when the connection degrades; the server logs its own view. */
export function describeIcePairs(report: Iterable<unknown>): string[] {
  type Candidate = { candidateType: string; address: string; port: number }
  type Pair = RTCIceCandidatePairStats & { selected?: boolean }
  type IceEntry = Partial<Candidate & Pair> & { id: string; type: string }
  const entries = Array.from(report) as IceEntry[]
  const byId = new Map(entries.map((entry) => [entry.id, entry]))
  const candidate = (id: string | undefined) => {
    const entry = id === undefined ? undefined : byId.get(id)
    if (!entry) return "?"
    return `${entry.candidateType ?? "?"} ${entry.address ?? "?"}:${entry.port ?? "?"}`
  }
  return entries
    .filter((entry) => entry.type === "candidate-pair")
    .map(
      (pair) =>
        `${candidate(pair.localCandidateId)} -> ${candidate(pair.remoteCandidateId)} ` +
        `${pair.state ?? "?"}${pair.nominated ? ",nominated" : ""}${pair.selected ? ",selected" : ""} ` +
        `rx ${pair.bytesReceived ?? 0}B tx ${pair.bytesSent ?? 0}B ` +
        `req ${pair.requestsSent ?? 0} resp ${pair.responsesReceived ?? 0} ` +
        `last rx ${pair.lastPacketReceivedTimestamp ?? "never"}`,
    )
}

function withoutKey<T>(record: Record<string, T>, key: string) {
  const next = { ...record }
  delete next[key]
  return next
}

/** The three simulcast encodings for a camera of `height` rows: the top layer is what the
 * room's quality policy allows, and each lower layer halves the resolution and quarters the
 * bitrate. Chrome requires the lowest layer first. */
export function simulcastEncodings(
  roomSize: number,
  quality: PublisherQuality,
  height: number | undefined,
): RTCRtpEncodingParameters[] {
  const top = videoEncoding(roomSize, quality, height)
  return SIMULCAST_LAYERS.map((rid, index) => {
    const steps = SIMULCAST_LAYERS.length - 1 - index
    return {
      rid,
      scaleResolutionDownBy: top.scaleResolutionDownBy * 2 ** steps,
      maxBitrate: Math.round(top.maxBitrate / 4 ** steps),
      maxFramerate: top.maxFramerate,
    }
  })
}

/** The layer worth decoding for a board drawn `height` device pixels tall: a rail tile gets
 * the quarter-resolution layer, a grid cell the half, and only the pinned board the full. */
export function layerForHeight(height: number): SimulcastLayer {
  if (height <= 270) return "l"
  if (height <= 540) return "m"
  return "h"
}

/** The rows of a 16:9 picture fitted inside a box, in device pixels. */
function fittedHeight(width: number, height: number) {
  return Math.min(height, (width * 9) / 16) * (globalThis.devicePixelRatio || 1)
}

/** This seat's one WebRTC connection to the server's SFU: its camera published as simulcast
 * layers, one incoming stream per other seat, the layer each board is worth at the size it
 * is drawn, and the per-seat messages (card crops) the server relays. Signaling travels over
 * the room channel in `link`. */
export function useSfuConnection(
  link: RoomLink,
  { stream, videoEnabled }: Pick<LocalCamera, "stream" | "videoEnabled">,
  quality: PublisherQuality,
  setError: (error: string) => void,
) {
  const connectionRef = useRef<Connection | null>(null)
  const listenersRef = useRef(new Set<PeerListener>())
  const qualityRef = useRef(quality)
  const iceServersRef = useRef<RTCIceServer[]>([])
  const [iceServers, setIceServersState] = useState<RTCIceServer[]>([])
  const [transport, setTransport] = useState<"direct" | "relay">("direct")
  const [streams, setStreams] = useState<Record<string, MediaStream>>({})
  const streamsRef = useRef(streams)
  streamsRef.current = streams
  const [connectionState, setConnectionState] = useState<RTCPeerConnectionState>("new")
  const [remoteIds, setRemoteIds] = useState<string[]>([])
  const revealToRef = useRef<string | null>(null)
  const [revealTo, setRevealTo] = useState<string | null>(null)
  const [revealBusy, setRevealBusy] = useState(false)
  /** The layer last asked of the server per board owner; a new subscription starts at "m". */
  const requestedLayersRef = useRef(new Map<string, SimulcastLayer>())
  const tilesRef = useRef(new Map<Element, Tile>())
  const observerRef = useRef<ResizeObserver | null>(null)

  /** Every remote seat shares the one connection, so they all show its state. */
  const connectionStates = useMemo(
    () => Object.fromEntries(remoteIds.map((id) => [id, connectionState])),
    [connectionState, remoteIds],
  )

  const syncVideo = useCallback(async () => {
    const connection = connectionRef.current
    if (!connection?.sender || !connection.videoTrack) return
    if (connection.pc.connectionState === "closed") return
    connection.videoTrack.enabled = videoEnabled()
    const parameters = connection.sender.getParameters()
    if (!parameters.encodings?.length) return
    const wanted = simulcastEncodings(
      Math.max(1, link.participants.length),
      qualityRef.current,
      stream()?.getVideoTracks()[0]?.getSettings().height,
    )
    // Chrome restarts the encoder on every setParameters, so only touch it on a change.
    const unchanged = parameters.encodings.every((current, index) => {
      const next = wanted[index] ?? wanted.at(-1)!
      return (
        current.scaleResolutionDownBy === next.scaleResolutionDownBy &&
        current.maxBitrate === next.maxBitrate &&
        current.maxFramerate === next.maxFramerate
      )
    })
    if (unchanged) return
    parameters.encodings = parameters.encodings.map((current, index) => {
      const { rid: _rid, ...next } = wanted[index] ?? wanted.at(-1)!
      return { ...current, ...next }
    })
    await connection.sender.setParameters(parameters)
  }, [link, stream, videoEnabled])

  const refreshVideo = useCallback(() => {
    void syncVideo().catch(() => setError("Could not update the camera's video layers."))
  }, [setError, syncVideo])

  useEffect(() => {
    qualityRef.current = quality
    refreshVideo()
  }, [quality, refreshVideo])

  /** Publishes a clone of the new camera track in place of the old one. */
  const replaceSourceTrack = useCallback(
    async (track: MediaStreamTrack) => {
      const connection = connectionRef.current
      if (!connection?.sender) return
      connection.videoTrack?.stop()
      connection.videoTrack = track.clone()
      connection.videoTrack.enabled = track.enabled
      await connection.sender.replaceTrack(connection.videoTrack)
      await syncVideo()
    },
    [syncVideo],
  )

  /** One stats report per board this seat receives, keyed by the board owner's peer id. */
  const getPeerStats = useCallback(async () => {
    const connection = connectionRef.current
    if (!connection) return []
    const receivers = connection.pc.getTransceivers().flatMap((transceiver) => {
      const owner = transceiver.mid ? connection.tracks[transceiver.mid] : undefined
      return owner && transceiver.receiver.track ? [{ owner, receiver: transceiver.receiver }] : []
    })
    return Promise.all(
      receivers.map(async ({ owner, receiver }) => ({
        id: owner,
        report: await receiver.getStats(),
      })),
    )
  }, [])

  const listen = useCallback((listener: PeerListener) => {
    listenersRef.current.add(listener)
    return () => {
      listenersRef.current.delete(listener)
    }
  }, [])

  /** Sends a message to one other seat through the server; false when the channel is down.
   * A refusal (the seat left, the message is over the relay's size cap) is logged, since the
   * other side only sees its request time out. */
  const send = useCallback(
    (peerId: string, message: DataMessage) => {
      const channel = link.channel
      if (!channel) return false
      channel
        .push("peer_message", { to: peerId, message })
        .receive("error", ({ reason }: { reason: string }) =>
          console.warn(`The table server refused a ${message.type} to another seat`, reason),
        )
      return true
    },
    [link],
  )

  /** A message another seat sent this one; anything malformed or unexpected is dropped. */
  const receivePeerMessage = useCallback(
    ({ from, message }: { from: string; message: unknown }) => {
      const parsed = parseDataMessage(message)
      if (!parsed) return
      for (const listener of listenersRef.current) listener.message?.(from, parsed)
    },
    [],
  )

  /** Asks the server for `layer` of `owner`'s board, once per change. */
  const requestLayer = useCallback(
    (owner: string, layer: SimulcastLayer) => {
      if (requestedLayersRef.current.get(owner) === layer) return
      requestedLayersRef.current.set(owner, layer)
      link.channel?.push("sfu_layer", { peer_id: owner, layer })
    },
    [link],
  )

  /** Re-derives every board's layer from the largest tile currently drawing it. */
  const updateLayers = useCallback(() => {
    const tallest = new Map<string, number>()
    for (const { streamId, height } of tilesRef.current.values()) {
      tallest.set(streamId, Math.max(tallest.get(streamId) ?? 0, height))
    }
    for (const [owner, remote] of Object.entries(streamsRef.current)) {
      const height = tallest.get(remote.id)
      if (height !== undefined) requestLayer(owner, layerForHeight(height))
    }
  }, [requestLayer])

  /** Follows the drawn size of a `<video>` showing `remote`; returns the unwatch. */
  const watchTile = useCallback(
    (remote: MediaStream, element: Element) => {
      tilesRef.current.set(element, { streamId: remote.id, height: 0 })
      if (typeof ResizeObserver !== "undefined") {
        observerRef.current ??= new ResizeObserver((entries) => {
          for (const entry of entries) {
            const tile = tilesRef.current.get(entry.target)
            if (tile) tile.height = fittedHeight(entry.contentRect.width, entry.contentRect.height)
          }
          updateLayers()
        })
        observerRef.current.observe(element)
      }
      return () => {
        observerRef.current?.unobserve(element)
        tilesRef.current.delete(element)
        updateLayers()
      }
    },
    [updateLayers],
  )

  /** The `<video>` on this page drawing `owner`'s board with a decoded frame, if any: a
   * clicker crops that frame itself when it is as sharp as the owner's camera. */
  const remoteFrame = useCallback((owner: string): HTMLVideoElement | null => {
    const streamId = streamsRef.current[owner]?.id
    if (!streamId) return null
    for (const [element, tile] of tilesRef.current) {
      if (tile.streamId !== streamId || !(element instanceof HTMLVideoElement)) continue
      if (element.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) return element
    }
    return null
  }, [])

  useEffect(() => {
    updateLayers()
  }, [streams, updateLayers])

  /** Queues one signaling step behind the previous ones. A step must check `current()` after
   * every await: the channel can rejoin (replacing the connection) while it waits. */
  const negotiate = useCallback(
    (connection: Connection, step: (current: () => boolean) => Promise<void>) => {
      const current = () =>
        connectionRef.current === connection && connection.pc.signalingState !== "closed"
      connection.negotiation = connection.negotiation.then(async () => {
        if (!current()) return
        try {
          await step(current)
        } catch (reason) {
          if (current()) console.warn("WebRTC negotiation with the table server failed", reason)
        }
      })
      return connection.negotiation
    },
    [],
  )

  /** Sends this seat's offer and resolves with the server's answer SDP. */
  const pushOffer = useCallback(
    (sdp: string) =>
      new Promise<string>((resolve, reject) => {
        const channel = link.channel
        if (!channel) return reject(new Error("channel closed"))
        channel
          .push("sfu_offer", { sdp })
          .receive("ok", (reply: { sdp: string }) => resolve(reply.sdp))
          .receive("error", ({ reason }: { reason: string }) => reject(new Error(reason)))
          .receive("timeout", () => reject(new Error("timeout")))
      }),
    [link],
  )

  /** Sends the answer to a server offer. Nothing waits on the acknowledgement: a rejected
   * answer means the server already moved on, and its next offer supersedes this one. */
  const pushAnswer = useCallback(
    (sdp: string) => {
      link.channel
        ?.push("sfu_answer", { sdp })
        .receive("error", ({ reason }: { reason: string }) =>
          console.warn("The table server rejected an answer", reason),
        )
    },
    [link],
  )

  const flushCandidates = useCallback(async (connection: Connection, current: () => boolean) => {
    for (const candidate of connection.candidates.splice(0)) {
      if (!current()) return
      await connection.pc.addIceCandidate(candidate)
    }
  }, [])

  /** Opens this seat's connection: its camera (or nothing, for a spectator) offered to the
   * server, which answers and then offers back every other board as it appears. */
  const connect = useCallback(() => {
    if (connectionRef.current) return
    const pc = new RTCPeerConnection({ iceServers: iceServersRef.current })
    const media = stream()
    let sender: RTCRtpSender | null = null
    let videoTrack: MediaStreamTrack | null = null
    if (!link.spectator && media?.getVideoTracks()[0]) {
      const source = media.getVideoTracks()[0]!
      videoTrack = source.clone()
      videoTrack.enabled = source.enabled
      const transceiver = pc.addTransceiver(videoTrack, {
        direction: "sendonly",
        streams: [media],
        sendEncodings: simulcastEncodings(
          Math.max(1, link.participants.length),
          qualityRef.current,
          source.getSettings().height,
        ),
      })
      sender = transceiver.sender
      const capabilities = globalThis.RTCRtpReceiver?.getCapabilities("video")
      if (capabilities && typeof transceiver.setCodecPreferences === "function")
        transceiver.setCodecPreferences(orderVideoCodecs(capabilities.codecs))
    } else {
      // A spectator still needs a media section for the transport to negotiate on.
      pc.addTransceiver("video", { direction: "recvonly" })
    }
    const connection: Connection = {
      pc,
      sender,
      videoTrack,
      negotiation: Promise.resolve(),
      candidates: [],
      tracks: {},
    }
    connectionRef.current = connection
    requestedLayersRef.current.clear()
    setConnectionState(pc.connectionState)
    pc.onicecandidate = ({ candidate }) => {
      if (candidate) link.channel?.push("sfu_candidate", { candidate: candidate.toJSON() })
    }
    pc.ontrack = ({ transceiver, streams: incoming, track }) => {
      const owner =
        (transceiver.mid ? connection.tracks[transceiver.mid] : undefined) ?? incoming[0]?.id
      if (!owner) return
      const remote = incoming[0] ?? new MediaStream([track])
      setStreams((current) =>
        current[owner] === remote ? current : { ...current, [owner]: remote },
      )
    }
    pc.onconnectionstatechange = () => {
      const state = pc.connectionState
      setConnectionState(state)
      if (state === "connected") refreshVideo()
      // The server sees the same failure and offers an ICE restart (answered like any other
      // offer); only if that keeps failing does it close the channel, which rejoins under a
      // new peer id. Log both sides' candidate pairs so a flaky path can be told apart from
      // a server that stopped answering.
      if (state === "failed" || state === "disconnected")
        void pc.getStats().then(
          (report) =>
            console.warn(`WebRTC connection to the table server ${state}`, {
              pairs: describeIcePairs(report.values()),
            }),
          () => console.warn(`WebRTC connection to the table server ${state}`),
        )
    }
    void negotiate(connection, async (current) => {
      const offer = await pc.createOffer()
      if (!current()) return
      await pc.setLocalDescription(offer)
      if (!current()) return
      const answer = await pushOffer(offer.sdp ?? "")
      if (!current()) return
      await pc.setRemoteDescription({ type: "answer", sdp: answer })
      await flushCandidates(connection, current)
    }).catch(() => {})
    connection.negotiation = connection.negotiation.then(() => {
      if (connectionRef.current === connection && !pc.remoteDescription)
        setError("Could not connect to the table's video server.")
    })
  }, [flushCandidates, link, negotiate, pushOffer, refreshVideo, setError, stream])

  /** A server offer: boards added or removed. Answered in order behind earlier steps. */
  const receiveOffer = useCallback(
    ({ sdp, tracks }: SfuOffer) => {
      const connection = connectionRef.current
      if (!connection) return
      void negotiate(connection, async (current) => {
        const { pc } = connection
        const owners = new Set(Object.values(tracks))
        for (const owner of owners) {
          if (!Object.values(connection.tracks).includes(owner))
            requestedLayersRef.current.delete(owner)
        }
        connection.tracks = tracks
        setStreams((currentStreams) =>
          Object.fromEntries(Object.entries(currentStreams).filter(([owner]) => owners.has(owner))),
        )
        await pc.setRemoteDescription({ type: "offer", sdp })
        await flushCandidates(connection, current)
        if (!current()) return
        const answer = await pc.createAnswer()
        if (!current()) return
        await pc.setLocalDescription(answer)
        if (current()) pushAnswer(answer.sdp ?? "")
      })
    },
    [flushCandidates, negotiate, pushAnswer],
  )

  const receiveCandidate = useCallback(
    ({ candidate }: { candidate: RTCIceCandidateInit }) => {
      const connection = connectionRef.current
      if (!connection) return
      void negotiate(connection, async () => {
        if (connection.pc.remoteDescription) await connection.pc.addIceCandidate(candidate)
        else connection.candidates.push(candidate)
      })
    },
    [negotiate],
  )

  const knownIdsRef = useRef(new Set<string>())

  /** Tracks who is present: forgets departed seats' video and tells listeners they left. */
  const syncPeers = useCallback(
    (everyone: TableParticipant[]) => {
      const activeIds = new Set(everyone.map((item) => item.peer_id))
      if (revealToRef.current && !activeIds.has(revealToRef.current)) {
        revealToRef.current = null
        setRevealTo(null)
      }
      for (const id of knownIdsRef.current) {
        if (activeIds.has(id)) continue
        knownIdsRef.current.delete(id)
        requestedLayersRef.current.delete(id)
        setStreams((current) => withoutKey(current, id))
        for (const listener of listenersRef.current) listener.left?.(id)
      }
      const remote: string[] = []
      for (const participant of everyone) {
        if (participant.peer_id === link.peerId) continue
        knownIdsRef.current.add(participant.peer_id)
        remote.push(participant.peer_id)
      }
      setRemoteIds((current) =>
        current.length === remote.length && current.every((id, index) => id === remote[index])
          ? current
          : remote,
      )
      refreshVideo()
    },
    [link, refreshVideo],
  )

  const closeAll = useCallback(() => {
    const connection = connectionRef.current
    if (!connection) return
    connection.pc.onconnectionstatechange = null
    connection.videoTrack?.stop()
    connection.pc.close()
    connectionRef.current = null
  }, [])

  /** A channel retry is a new media generation: drop the connection and every board's video. */
  const reset = useCallback(() => {
    closeAll()
    knownIdsRef.current.clear()
    requestedLayersRef.current.clear()
    setStreams({})
    setRemoteIds([])
    setConnectionState("new")
  }, [closeAll])

  useEffect(
    () => () => {
      observerRef.current?.disconnect()
    },
    [],
  )

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

  /** The server stops forwarding this seat's video to everyone but `target`. */
  const changeReveal = useCallback(
    async (target: string | null) => {
      const channel = link.channel
      if (revealBusy || !channel) return
      setRevealBusy(true)
      revealToRef.current = target
      setRevealTo(target)
      try {
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
    [link, revealBusy, setError],
  )

  return {
    streams,
    connectionStates,
    iceServers,
    transport,
    setTransport,
    revealTo,
    revealBusy,
    refreshVideo,
    replaceSourceTrack,
    getPeerStats,
    listen,
    send,
    connect,
    receiveOffer,
    receiveCandidate,
    receivePeerMessage,
    watchTile,
    remoteFrame,
    syncPeers,
    reset,
    closeAll,
    setIceServers,
    revealTarget,
    restoreReveal,
    changeReveal,
  }
}

export type SfuConnection = ReturnType<typeof useSfuConnection>
