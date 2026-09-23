import { Channel, Presence, Socket } from "phoenix"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { api } from "@/lib/api"
import { mergeIdentifiedCards, sameCard } from "./identified-cards"
import { canViewBoard, videoEncoding } from "./media-policy"
import type { GalleryArt } from "./recognition/pipeline"
import {
  EMPTY_COUNTERS,
  changeCounter,
  describeCounterChanges,
  type Counter,
  type SeatCounters,
} from "./seat-counters"
import {
  describeParticipantChange,
  describeParticipantLeft,
  orderBySeats,
  shuffleSeats,
} from "./table-events"

export interface TableParticipant extends SeatCounters {
  peer_id: string
  player_id: number
  player_name: string
  life: number
  /** Server clock (ms) when the seat was taken; default seat order is join order. */
  joined_at: number
  camera_off: boolean
  reveal_to?: string | null
  deck_id?: number
  deck_name?: string
}

/** Status a player publishes about their own seat; mirrors the channel's `update_status`. */
export type SeatStatus = Partial<Pick<TableParticipant, "life" | "camera_off"> & SeatCounters>

interface Monarch {
  peer_id: string
  player_name: string
}

interface MonarchEvent {
  holder: Monarch | null
  revision: number
}

export interface TableEvent {
  id: number
  at: Date
  text: string
}

interface TableConfig {
  ice_servers: RTCIceServer[]
  max_players: number
  minimum_height: number
  socket_token: string
}

export interface CapturedCard {
  peerId: string
  playerId: number
  /** JPEG data URL of the native crop around the click. */
  image: string
  nativeWidth: number
  nativeHeight: number
  /** Side of the square crop in native pixels (640 unless the camera is smaller). */
  cropSize: number
  /** The click in crop pixels; the crop is clamped to the frame so it is not always centred. */
  clickX: number
  clickY: number
  /** Shift+click: the clicker wants to see and choose among the candidates even when the
   * recognizer is sure. A plain click logs a clear answer silently. */
  inspect: boolean
  /** Keep reveal captures private even if identification finishes after the reveal ends. */
  private: boolean
}

/** A card a seat named on someone's board, recognized or picked by hand. */
export type IdentifiedCard = Pick<GalleryArt, "id" | "name" | "set" | "collector_number">

/** One entry in the shared per-board list of identified cards. Ephemeral like the Log: it
 * lives on the data channels and is synced to seats that connect later. */
export interface BoardCard {
  id: string
  ownerPeerId: string
  byPlayerName: string
  card: IdentifiedCard
  /** Clicker's clock (ms); only used to order the list. */
  at: number
}

type Signal = { description: RTCSessionDescriptionInit } | { candidate: RTCIceCandidateInit }

type DataMessage =
  | { type: "capture_request"; requestId: string; x: number; y: number }
  | {
      type: "capture_response"
      requestId: string
      image: string
      nativeWidth: number
      nativeHeight: number
      cropSize: number
      clickX: number
      clickY: number
      private: boolean
    }
  | { type: "deck_suggestion"; deckId: number }
  | { type: "card_identified"; entry: BoardCard }
  | { type: "card_removed"; id: string }
  /** Sent when a data channel opens so a late joiner sees the cards already on the table. */
  | { type: "cards_sync"; entries: BoardCard[] }

interface PeerState {
  connection: RTCPeerConnection
  videoSender: RTCRtpSender
  videoTrack: MediaStreamTrack
  mediaUpdate: Promise<void>
  channel?: RTCDataChannel
  stream?: MediaStream
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

const CROP_SIZE = 640
export const STARTING_LIFE = 40

function captureCrop(video: HTMLVideoElement, x: number, y: number) {
  const width = video.videoWidth
  const height = video.videoHeight
  const size = Math.min(CROP_SIZE, width, height)
  const left = Math.max(0, Math.min(width - size, Math.round(x * width - size / 2)))
  const top = Math.max(0, Math.min(height - size, Math.round(y * height - size / 2)))
  const canvas = document.createElement("canvas")
  canvas.width = size
  canvas.height = size
  canvas.getContext("2d")?.drawImage(video, left, top, size, size, 0, 0, size, size)
  return {
    image: canvas.toDataURL("image/jpeg", 0.82),
    nativeWidth: width,
    nativeHeight: height,
    cropSize: size,
    clickX: x * width - left,
    clickY: y * height - top,
  }
}

export function useWebcamRoom(roomId: string, playerId: number, deckId: number | null) {
  const peerIdRef = useRef(crypto.randomUUID())
  const localVideoRef = useRef<HTMLVideoElement | null>(null)
  const localStreamRef = useRef<MediaStream | null>(null)
  const channelRef = useRef<Channel | null>(null)
  const peersRef = useRef(new Map<string, PeerState>())
  const participantsRef = useRef<TableParticipant[]>([])
  const pendingCaptures = useRef(new Map<string, { targetPeerId: string; inspect: boolean }>())
  const eventIdRef = useRef(0)
  const [participants, setParticipants] = useState<TableParticipant[]>([])
  const [seatOrder, setSeatOrder] = useState<string[]>([])
  const [events, setEvents] = useState<TableEvent[]>([])
  const [streams, setStreams] = useState<Record<string, MediaStream>>({})
  const [connectionStates, setConnectionStates] = useState<Record<string, RTCPeerConnectionState>>(
    {},
  )
  const [iceServers, setIceServers] = useState<RTCIceServer[]>([])
  const [localStream, setLocalStream] = useState<MediaStream | null>(null)
  const [cameraOff, setCameraOff] = useState(false)
  const revealToRef = useRef<string | null>(null)
  const [revealTo, setRevealTo] = useState<string | null>(null)
  const [revealBusy, setRevealBusy] = useState(false)
  // Your own life is tracked locally so rapid ± clicks compound before presence
  // echoes the new total back; presence stays the source for everyone else.
  const lifeRef = useRef(STARTING_LIFE)
  const [life, setLifeState] = useState(STARTING_LIFE)
  const countersRef = useRef(EMPTY_COUNTERS)
  const [counters, setCounters] = useState(EMPTY_COUNTERS)
  const [monarch, setMonarch] = useState<Monarch | null>(null)
  const [capture, setCapture] = useState<CapturedCard | null>(null)
  const cardsRef = useRef<BoardCard[]>([])
  const [identifiedCards, setIdentifiedCards] = useState<BoardCard[]>([])
  const [status, setStatus] = useState("Opening 1080p camera…")
  const [error, setError] = useState<string | null>(null)

  const log = useCallback((lines: string[]) => {
    if (lines.length === 0) return
    const at = new Date()
    setEvents((current) =>
      [...lines.map((text) => ({ id: (eventIdRef.current += 1), at, text })), ...current].slice(
        0,
        200,
      ),
    )
  }, [])

  const chooseDeck = useCallback((chosenDeckId: number) => {
    channelRef.current?.push("choose_deck", { deck_id: chosenDeckId })
  }, [])

  const updateStatus = useCallback((changes: SeatStatus) => {
    channelRef.current?.push("update_status", changes)
  }, [])

  // Disable private clones synchronously, then detach senders. Never disable the shared
  // native track to hide just one peer: the target and native crop RPC still need it.
  const syncVideo = useCallback(() => {
    const updates = [...peersRef.current].map(([id, peer]) => {
      const visible = canViewBoard(peerIdRef.current, id, revealToRef.current)
      peer.videoTrack.enabled = visible && !!localStreamRef.current?.getVideoTracks()[0]?.enabled
      peer.mediaUpdate = peer.mediaUpdate
        .catch(() => {})
        .then(async () => {
          if (peer.connection.connectionState === "closed") return
          const allowed = canViewBoard(peerIdRef.current, id, revealToRef.current)
          await peer.videoSender.replaceTrack(allowed ? peer.videoTrack : null)
          const parameters = peer.videoSender.getParameters()
          if (parameters.encodings?.length) {
            const encoding = videoEncoding(participantsRef.current.length)
            parameters.encodings = parameters.encodings.map((current) => ({
              ...current,
              ...encoding,
            }))
            await peer.videoSender.setParameters(parameters)
          }
        })
      return peer.mediaUpdate
    })
    return Promise.all(updates)
  }, [])

  const refreshVideo = useCallback(() => {
    void syncVideo().catch(() =>
      setError("Could not update video senders. Hidden cameras remain blocked."),
    )
  }, [syncVideo])

  /** Every ingress uses the same per-board card identity rule, including late-join syncs. */
  const mergeCards = useCallback((entries: BoardCard[]) => {
    cardsRef.current = mergeIdentifiedCards(cardsRef.current, entries)
    setIdentifiedCards(cardsRef.current)
  }, [])

  const dropCard = useCallback((id: string) => {
    cardsRef.current = cardsRef.current.filter((entry) => entry.id !== id)
    setIdentifiedCards(cardsRef.current)
  }, [])

  const broadcast = useCallback((message: DataMessage) => {
    const payload = JSON.stringify(message)
    for (const peer of peersRef.current.values()) {
      if (peer.channel?.readyState === "open") peer.channel.send(payload)
    }
  }, [])

  const handleData = useCallback(
    (fromPeerId: string, event: MessageEvent<string>) => {
      const message = JSON.parse(event.data) as DataMessage
      if (message.type === "capture_request" && localVideoRef.current) {
        if (
          !canViewBoard(peerIdRef.current, fromPeerId, revealToRef.current) ||
          !localStreamRef.current?.getVideoTracks()[0]?.enabled
        )
          return
        const result = captureCrop(localVideoRef.current, message.x, message.y)
        peersRef.current.get(fromPeerId)?.channel?.send(
          JSON.stringify({
            type: "capture_response",
            requestId: message.requestId,
            private: !!revealToRef.current,
            ...result,
          }),
        )
      } else if (message.type === "capture_response") {
        const pending = pendingCaptures.current.get(message.requestId)
        pendingCaptures.current.delete(message.requestId)
        const owner = participantsRef.current.find((item) => item.peer_id === fromPeerId)
        if (
          owner &&
          pending?.targetPeerId === fromPeerId &&
          canViewBoard(fromPeerId, peerIdRef.current, owner.reveal_to) &&
          !owner.camera_off
        )
          setCapture({
            peerId: fromPeerId,
            playerId: owner.player_id,
            inspect: pending.inspect,
            ...message,
          })
      } else if (message.type === "deck_suggestion") {
        chooseDeck(message.deckId)
      } else if (message.type === "card_identified") {
        mergeCards([message.entry])
      } else if (message.type === "cards_sync") {
        mergeCards(message.entries)
      } else if (message.type === "card_removed") {
        dropCard(message.id)
      }
    },
    [chooseDeck, dropCard, mergeCards],
  )

  useEffect(() => {
    let disposed = false
    let socket: Socket | null = null
    let presence: Presence | null = null
    let monarchRevision = 0

    function syncMonarch({ holder, revision }: MonarchEvent) {
      if (revision <= monarchRevision) return
      monarchRevision = revision
      setMonarch(holder)
    }

    function attachDataChannel(peerId: string, dataChannel: RTCDataChannel) {
      const peer = peersRef.current.get(peerId)
      if (!peer) return
      peer.channel = dataChannel
      dataChannel.onmessage = (event) => handleData(peerId, event)
      // Incoming channels can already be open when announced, so sync in both cases.
      const syncCards = () => {
        if (cardsRef.current.length > 0)
          dataChannel.send(JSON.stringify({ type: "cards_sync", entries: cardsRef.current }))
      }
      if (dataChannel.readyState === "open") syncCards()
      else dataChannel.onopen = syncCards
    }

    function sendSignal(target: string, signal: Signal) {
      channelRef.current?.push("signal", { target, signal })
    }

    function createPeer(remotePeerId: string, config: TableConfig) {
      const existing = peersRef.current.get(remotePeerId)
      if (existing) return existing
      const connection = new RTCPeerConnection({ iceServers: config.ice_servers })
      const media = localStreamRef.current as MediaStream
      const videoTrack = media.getVideoTracks()[0]!.clone()
      const visible = canViewBoard(peerIdRef.current, remotePeerId, revealToRef.current)
      videoTrack.enabled = visible && media.getVideoTracks()[0]!.enabled
      // addTrack lets an incoming offer reuse this transceiver on the answering side.
      const videoSender = connection.addTrack(videoTrack, media)
      const peer: PeerState = {
        connection,
        videoSender,
        videoTrack,
        mediaUpdate: visible ? Promise.resolve() : videoSender.replaceTrack(null),
        candidates: [],
        restarted: false,
      }
      peersRef.current.set(remotePeerId, peer)
      connection.onicecandidate = ({ candidate }) => {
        if (candidate) sendSignal(remotePeerId, { candidate: candidate.toJSON() })
      }
      connection.ontrack = ({ streams: incoming }) => {
        const stream = incoming[0]
        if (!stream) return
        peer.stream = stream
        setStreams((current) => ({ ...current, [remotePeerId]: stream }))
      }
      connection.ondatachannel = ({ channel: incoming }) =>
        attachDataChannel(remotePeerId, incoming)
      connection.onconnectionstatechange = () => {
        const state = connection.connectionState
        if (state === "connected") refreshVideo()
        setConnectionStates((current) => ({ ...current, [remotePeerId]: state }))
        if (state === "failed" || state === "closed") {
          setStreams((current) => {
            const next = { ...current }
            delete next[remotePeerId]
            return next
          })
        }
        // One ICE restart covers a transient path loss; the side that made the first offer
        // makes the new one. If the networks simply cannot reach each other (no TURN), the
        // second failure stays on screen so the seat knows why.
        if (state === "failed" && !peer.restarted && peerIdRef.current < remotePeerId) {
          peer.restarted = true
          connection.restartIce()
          void connection.createOffer().then(async (offer) => {
            await connection.setLocalDescription(offer)
            sendSignal(remotePeerId, { description: offer })
          })
        }
      }
      return peer
    }

    async function run() {
      try {
        const config = await api<{ data: TableConfig }>("/api/webcam-table/config").then(
          (body) => body.data,
        )
        setIceServers(config.ice_servers)
        const media = await navigator.mediaDevices.getUserMedia({
          video: {
            width: { min: 1920, ideal: 1920 },
            height: { min: 1080, ideal: 1080 },
            frameRate: { ideal: 30 },
          },
          audio: false,
        })
        if (disposed) return media.getTracks().forEach((track) => track.stop())
        localStreamRef.current = media
        setLocalStream(media)
        const captureVideo = document.createElement("video")
        captureVideo.muted = true
        captureVideo.playsInline = true
        captureVideo.srcObject = media
        await captureVideo.play()
        localVideoRef.current = captureVideo

        socket = new Socket("/socket", { params: { token: config.socket_token } })
        socket.connect()
        const room = socket.channel(`webcam_table:${roomId}`, {
          peer_id: peerIdRef.current,
          player_id: playerId,
          deck_id: deckId,
        })
        channelRef.current = room
        presence = new Presence(room)
        presence.onJoin((_id, current, joined) => {
          const previous = current?.metas[0] as TableParticipant | undefined
          const next = joined.metas[0] as TableParticipant | undefined
          if (next) log(describeParticipantChange(previous, next))
          if (previous && next)
            log(describeCounterChanges(previous, next, next.player_name, participantsRef.current))
        })
        presence.onLeave((_id, current, left) => {
          const participant = left.metas[0] as TableParticipant | undefined
          if (participant && current.metas.length === 0) log([describeParticipantLeft(participant)])
        })
        room.on("seat_order", ({ peer_ids }: { peer_ids: string[] }) => {
          setSeatOrder(peer_ids)
          log(["Seat order randomized"])
        })
        room.on("monarch_state", syncMonarch)
        room.on("monarch", (event: MonarchEvent) => {
          syncMonarch(event)
          const { holder } = event
          log([holder ? `${holder.player_name} took the monarch` : "The monarch left the table"])
        })
        presence.onSync(() => {
          const next = presence?.list((_id, value) => value.metas[0] as TableParticipant) ?? []
          participantsRef.current = next
          setParticipants(next)
          const activeIds = new Set(next.map((item) => item.peer_id))
          if (revealToRef.current && !activeIds.has(revealToRef.current)) {
            revealToRef.current = null
            setRevealTo(null)
          }
          peersRef.current.forEach((peer, id) => {
            if (!activeIds.has(id)) {
              peer.videoTrack.stop()
              peer.connection.close()
              peersRef.current.delete(id)
              setConnectionStates((current) => {
                const rest = { ...current }
                delete rest[id]
                return rest
              })
            }
          })
          for (const participant of next) {
            const remoteId = participant.peer_id
            if (remoteId === peerIdRef.current || peersRef.current.has(remoteId)) continue
            const peer = createPeer(remoteId, config)
            if (peerIdRef.current < remoteId) {
              const dataChannel = peer.connection.createDataChannel("table")
              attachDataChannel(remoteId, dataChannel)
              void peer.connection.createOffer().then(async (offer) => {
                await peer.connection.setLocalDescription(offer)
                sendSignal(remoteId, { description: offer })
              })
            }
          }
          refreshVideo()
        })
        room.on(
          "signal",
          async ({ target, from, signal }: { target: string; from: string; signal: Signal }) => {
            if (target !== peerIdRef.current) return
            const peer = createPeer(from, config)
            if ("description" in signal) {
              await peer.connection.setRemoteDescription(signal.description)
              for (const candidate of peer.candidates.splice(0)) {
                await peer.connection.addIceCandidate(candidate)
              }
              if (signal.description.type === "offer") {
                const answer = await peer.connection.createAnswer()
                await peer.connection.setLocalDescription(answer)
                sendSignal(from, { description: answer })
              }
            } else if (peer.connection.remoteDescription) {
              await peer.connection.addIceCandidate(signal.candidate)
            } else {
              peer.candidates.push(signal.candidate)
            }
          },
        )
        room
          .join()
          .receive("ok", () => {
            // A server restart also restarts the monotonic revision clock.
            monarchRevision = 0
            setStatus("Live — click any board to inspect a card")
            // Presence starts every (re)join at the defaults; republish what this seat knows.
            room.push("update_status", {
              life: lifeRef.current,
              ...countersRef.current,
              camera_off: !(localStreamRef.current?.getVideoTracks()[0]?.enabled ?? true),
            })
            if (revealToRef.current) room.push("reveal", { target: revealToRef.current })
          })
          .receive("error", ({ reason }: { reason: string }) => setError(reason))
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : "Could not start the webcam table")
      }
    }

    void run()
    return () => {
      disposed = true
      channelRef.current?.leave()
      socket?.disconnect()
      peersRef.current.forEach((peer) => {
        peer.videoTrack.stop()
        peer.connection.close()
      })
      peersRef.current.clear()
      localStreamRef.current?.getTracks().forEach((track) => track.stop())
    }
  }, [deckId, handleData, log, playerId, refreshVideo, roomId])

  async function changeReveal(target: string | null) {
    if (revealBusy || !channelRef.current) return
    setRevealBusy(true)
    revealToRef.current = target
    setRevealTo(target)
    try {
      await syncVideo()
      await new Promise<void>((resolve, reject) => {
        channelRef
          .current!.push("reveal", { target })
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
  }

  function changeLife(delta: number) {
    const next = Math.max(-999, Math.min(999, lifeRef.current + delta))
    lifeRef.current = next
    setLifeState(next)
    updateStatus({ life: next })
  }

  function adjustCounter(counter: Counter, delta: number) {
    const next = changeCounter(countersRef.current, counter, delta)
    countersRef.current = next
    setCounters(next)
    updateStatus(next)
  }

  function takeMonarch() {
    channelRef.current?.push("take_monarch", {})
  }

  function toggleCamera() {
    const next = !cameraOff
    localStreamRef.current?.getVideoTracks().forEach((track) => {
      track.enabled = !next
    })
    refreshVideo()
    setCameraOff(next)
    updateStatus({ camera_off: next })
  }

  function randomizeSeats() {
    const current = orderBySeats(participantsRef.current, seatOrder).map((item) => item.peer_id)
    channelRef.current?.push("seat_order", { peer_ids: shuffleSeats(current) })
  }

  function requestCapture(targetPeerId: string, x: number, y: number, inspect = false) {
    const owner = participantsRef.current.find((item) => item.peer_id === targetPeerId)
    if (
      !owner ||
      owner.camera_off ||
      !canViewBoard(targetPeerId, peerIdRef.current, owner.reveal_to)
    )
      return
    if (targetPeerId === peerIdRef.current && localVideoRef.current) {
      const result = captureCrop(localVideoRef.current, x, y)
      setCapture({
        peerId: targetPeerId,
        playerId,
        inspect,
        private: !!revealToRef.current,
        ...result,
      })
      return
    }
    const requestId = crypto.randomUUID()
    pendingCaptures.current.set(requestId, { targetPeerId, inspect })
    peersRef.current
      .get(targetPeerId)
      ?.channel?.send(JSON.stringify({ type: "capture_request", requestId, x, y }))
    setStatus("Requesting native camera crop…")
  }

  /** Names a card on `ownerPeerId`'s board: added to that board's card list here and at
   * every other seat. The capture stays current so the clicker can still say "wrong card" and
   * pick again from the same crop; the page dismisses it when it is done with the result. */
  function announceCard(ownerPeerId: string, byPlayerName: string, card: IdentifiedCard) {
    const existing = cardsRef.current.find(
      (entry) => entry.ownerPeerId === ownerPeerId && sameCard(entry.card, card),
    )
    if (existing) return existing
    const entry: BoardCard = {
      id: crypto.randomUUID(),
      ownerPeerId,
      byPlayerName,
      card,
      at: Date.now(),
    }
    // Identifying a private hand must not publish its card names to the table's shared tray.
    const owner = participantsRef.current.find((item) => item.peer_id === ownerPeerId)
    if (
      owner?.reveal_to ||
      (ownerPeerId === peerIdRef.current && revealToRef.current) ||
      (capture?.peerId === ownerPeerId && capture.private)
    )
      return entry
    mergeCards([entry])
    broadcast({ type: "card_identified", entry })
    return entry
  }

  /** Takes a misidentified card off its board's list at every seat. */
  function removeCard(id: string) {
    dropCard(id)
    broadcast({ type: "card_removed", id })
  }

  function suggestDeck(targetPeerId: string, suggestedDeckId: number) {
    if (targetPeerId === peerIdRef.current) chooseDeck(suggestedDeckId)
    else
      peersRef.current
        .get(targetPeerId)
        ?.channel?.send(JSON.stringify({ type: "deck_suggestion", deckId: suggestedDeckId }))
    setCapture(null)
  }

  /** Participants in shared seat order; the End game form records seats in this order. */
  const seatedParticipants = useMemo(
    () =>
      orderBySeats(participants, seatOrder).map((participant) =>
        participant.peer_id === peerIdRef.current
          ? { ...participant, life, ...counters }
          : participant,
      ),
    [counters, life, participants, seatOrder],
  )

  return {
    peerId: peerIdRef.current,
    participants: seatedParticipants,
    events,
    streams,
    connectionStates,
    iceServers,
    localStream,
    cameraOff,
    revealTo,
    revealBusy,
    changeReveal,
    capture,
    identifiedCards,
    status,
    error,
    requestCapture,
    suggestDeck,
    announceCard,
    removeCard,
    chooseDeck,
    life,
    changeLife,
    counters,
    adjustCounter,
    monarch,
    takeMonarch,
    toggleCamera,
    randomizeSeats,
    dismissCapture: () => setCapture(null),
  }
}
