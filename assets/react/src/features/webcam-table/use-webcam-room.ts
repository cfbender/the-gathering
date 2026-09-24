import { useQueryClient } from "@tanstack/react-query"
import { Channel, Presence, Socket } from "phoenix"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { api } from "@/lib/api"
import type { GameFormat } from "@/features/games/game-format"
import { openCamera } from "./camera"
import { applyCardCommand, sameCard, type CardCommand } from "./identified-cards"
import { canViewBoard, videoEncoding, type PublisherQuality } from "./media-policy"
import type { GameTimerState, TimerSample } from "./game-timer"
import type { GalleryArt } from "./recognition/pipeline"
import { sharesCorrections } from "./use-correction-upload"
import {
  EMPTY_COUNTERS,
  changeCounter,
  describeCounterChanges,
  type Counter,
  type SeatCounters,
} from "./seat-counters"
import {
  appendTableEvent,
  describeParticipantChange,
  describeParticipantLeft,
  orderBySeats,
  retainEliminatedSeats,
  type TableEvent,
  type TableEventContent,
} from "./table-events"
import { describeRoll, type RollRequest, type TableRoll } from "./table-rolls"
import { EMPTY_TURNS, type TurnState } from "./turns"

export type { TableEvent } from "./table-events"

export interface TableParticipant extends SeatCounters {
  peer_id: string
  player_id: number
  player_name: string
  life: number
  /** Server clock (ms) when the seat was taken; default seat order is join order. */
  joined_at: number
  camera_off: boolean
  reveal_to?: string | null
  eliminated: boolean
  spectator?: boolean
  /** Retained result seat after an eliminated player disconnects. */
  departed?: boolean
  deck_id?: number
  deck_name?: string
}

/** Status a player publishes about their own seat; mirrors the channel's `update_status`. */
export type SeatStatus = Partial<
  Pick<TableParticipant, "life" | "camera_off" | "eliminated"> & SeatCounters
>

interface Monarch {
  peer_id: string
  player_name: string
}

interface MonarchEvent {
  holder: Monarch | null
  revision: number
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
  /** Camera owner's consent, carried with the crop; absent older peers do not opt in. */
  shareCorrections?: boolean
  /** Shift+click: the clicker wants to see and choose among the candidates even when the
   * recognizer is sure. A plain click logs a clear answer silently. */
  inspect: boolean
  /** Keep reveal captures private even if identification finishes after the reveal ends. */
  private: boolean
}

/** A card a seat named on someone's board, recognized or picked by hand. */
export type IdentifiedCard = Pick<GalleryArt, "id" | "name" | "set" | "collector_number">

/** One entry in the shared per-board list of identified cards. The server owns the list: it
 * validates every change and broadcasts the whole list, including to seats that join later. */
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
      shareCorrections?: boolean
    }

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
    shareCorrections: sharesCorrections(),
  }
}

export function useWebcamRoom(
  roomId: string,
  playerId: number,
  deckId: number | null,
  deviceId = "",
  quality: PublisherQuality = "auto",
  cameraEnabled = true,
) {
  const queryClient = useQueryClient()
  const deviceIdRef = useRef(deviceId)
  const qualityRef = useRef(quality)
  const cameraRequest = useRef(0)
  const cameraChangingRef = useRef(false)
  const [cameraChanging, setCameraChanging] = useState(false)
  const [cameraError, setCameraError] = useState<string | null>(null)
  const peerIdRef = useRef(crypto.randomUUID())
  const localVideoRef = useRef<HTMLVideoElement | null>(null)
  const localStreamRef = useRef<MediaStream | null>(null)
  const channelRef = useRef<Channel | null>(null)
  const peersRef = useRef(new Map<string, PeerState>())
  const participantsRef = useRef<TableParticipant[]>([])
  const pendingCaptures = useRef(new Map<string, { targetPeerId: string; inspect: boolean }>())
  const eventIdRef = useRef(0)
  const [participants, setParticipants] = useState<TableParticipant[]>([])
  const [eliminatedSeats, setEliminatedSeats] = useState<TableParticipant[]>([])
  const [seatOrder, setSeatOrder] = useState<string[]>([])
  const [shuffleVersion, setShuffleVersion] = useState(0)
  const [timer, setTimer] = useState<TimerSample | null>(null)
  const [turns, setTurns] = useState<TurnState>(EMPTY_TURNS)
  const [mode, setModeState] = useState<GameFormat>("commander")
  const [teamLife, setTeamLife] = useState<Record<number, number>>({})
  const [spectating, setSpectating] = useState(false)
  const spectatorRef = useRef(false)
  const [ownerId, setOwnerId] = useState<number | null>(null)
  const [roll, setRoll] = useState<TableRoll | null>(null)
  const [events, setEvents] = useState<TableEvent[]>([])
  const [streams, setStreams] = useState<Record<string, MediaStream>>({})
  const [connectionStates, setConnectionStates] = useState<Record<string, RTCPeerConnectionState>>(
    {},
  )
  const [iceServers, setIceServers] = useState<RTCIceServer[]>([])
  const [localStream, setLocalStream] = useState<MediaStream | null>(null)
  const cameraOffRef = useRef(!cameraEnabled)
  const [cameraOff, setCameraOff] = useState(!cameraEnabled)
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
  const serverCardsRef = useRef<BoardCard[]>([])
  const pendingCardsRef = useRef(new Map<number, CardCommand>())
  const cardCommandRef = useRef(0)
  const [identifiedCards, setIdentifiedCards] = useState<BoardCard[]>([])
  const [status, setStatus] = useState("Opening 1080p camera…")
  const [error, setError] = useState<string | null>(null)

  const log = useCallback((lines: (string | TableEventContent)[]) => {
    if (lines.length === 0) return
    const at = new Date()
    const entries = lines.map((line) => ({
      ...(typeof line === "string" ? { text: line } : line),
      id: (eventIdRef.current += 1),
      at,
    }))
    setEvents((current) => entries.reduce(appendTableEvent, current))
  }, [])

  useEffect(() => {
    if (!roll) return
    const timeout = window.setTimeout(() => setRoll(null), 5000)
    return () => window.clearTimeout(timeout)
  }, [roll])

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
      const visible =
        !spectatorRef.current && canViewBoard(peerIdRef.current, id, revealToRef.current)
      peer.videoTrack.enabled = visible && !!localStreamRef.current?.getVideoTracks()[0]?.enabled
      peer.mediaUpdate = peer.mediaUpdate
        .catch(() => {})
        .then(async () => {
          if (peer.connection.connectionState === "closed") return
          const allowed =
            !spectatorRef.current && canViewBoard(peerIdRef.current, id, revealToRef.current)
          await peer.videoSender.replaceTrack(allowed ? peer.videoTrack : null)
          const parameters = peer.videoSender.getParameters()
          if (parameters.encodings?.length) {
            const encoding = videoEncoding(
              participantsRef.current.length,
              qualityRef.current,
              localStreamRef.current?.getVideoTracks()[0]?.getSettings().height,
            )
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

  useEffect(() => {
    qualityRef.current = quality
    refreshVideo()
  }, [quality, refreshVideo])

  const getPeerStats = useCallback(async () => {
    return Promise.all(
      [...peersRef.current].map(async ([id, peer]) => ({
        id,
        report: await peer.connection.getStats(),
      })),
    )
  }, [])

  async function changeCamera(nextDeviceId: string): Promise<boolean> {
    if (spectatorRef.current || cameraChangingRef.current || !localStreamRef.current) return false
    cameraChangingRef.current = true
    setCameraChanging(true)
    setCameraError(null)
    const request = ++cameraRequest.current
    try {
      const media = await openCamera(nextDeviceId).catch((reason: unknown) => {
        if (
          nextDeviceId &&
          reason instanceof DOMException &&
          ["NotFoundError", "OverconstrainedError"].includes(reason.name)
        ) {
          setCameraError("Saved camera is unavailable; using the system default.")
          return openCamera("")
        }
        throw reason
      })
      if (request !== cameraRequest.current) {
        media.getTracks().forEach((track) => track.stop())
        return false
      }
      const previous = localStreamRef.current
      const track = media.getVideoTracks()[0]!
      track.enabled = previous?.getVideoTracks()[0]?.enabled ?? false
      localStreamRef.current = media
      setLocalStream(media)
      if (localVideoRef.current) {
        // Assigning srcObject pauses the element; left paused, the hidden capture video
        // would freeze on the camera's first frame and every crop would repeat it.
        localVideoRef.current.srcObject = media
        void localVideoRef.current.play().catch(() => {})
      }
      // Stop old clones before replacing, including clones detached by a private reveal.
      // syncVideo rechecks current consent inside each sender's serialized update.
      for (const [id, peer] of peersRef.current) {
        peer.videoTrack.stop()
        peer.videoTrack = track.clone()
        peer.videoTrack.enabled =
          track.enabled && canViewBoard(peerIdRef.current, id, revealToRef.current)
      }
      previous?.getTracks().forEach((oldTrack) => oldTrack.stop())
      deviceIdRef.current = nextDeviceId
      await syncVideo().catch(() => {
        setCameraError(
          "Camera changed, but a peer's video could not be updated. Try switching again.",
        )
      })
      return true
    } catch (reason) {
      setCameraError(reason instanceof Error ? reason.message : "Could not switch camera")
      return false
    } finally {
      cameraChangingRef.current = false
      setCameraChanging(false)
    }
  }

  /** Shows the server's list with this seat's unanswered commands applied on top. */
  const showCards = useCallback(() => {
    cardsRef.current = [...pendingCardsRef.current.values()].reduce(
      applyCardCommand,
      serverCardsRef.current,
    )
    setIdentifiedCards(cardsRef.current)
  }, [])

  const receiveCards = useCallback(
    (entries: BoardCard[]) => {
      serverCardsRef.current = entries
      showCards()
    },
    [showCards],
  )

  /** Sends a card change to the server and shows it until the server answers. The server
   * broadcasts its list before replying, so an accepted change never flickers; a rejected or
   * timed-out change falls back to the server's list. */
  const sendCardCommand = useCallback(
    (command: CardCommand) => {
      const channel = channelRef.current
      if (spectatorRef.current || !channel) return
      const id = (cardCommandRef.current += 1)
      pendingCardsRef.current.set(id, command)
      showCards()
      const settle = () => {
        pendingCardsRef.current.delete(id)
        showCards()
      }
      channel
        .push("cards", command)
        .receive("ok", settle)
        .receive("error", settle)
        .receive("timeout", settle)
    },
    [showCards],
  )

  const handleData = useCallback((fromPeerId: string, event: MessageEvent<string>) => {
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
    }
  }, [])

  useEffect(() => {
    let disposed = false
    let socket: Socket | null = null
    let presence: Presence | null = null
    let monarchRevision = -1
    let cameraStarted = false

    function syncMonarch({ holder, revision }: MonarchEvent) {
      if (revision < monarchRevision) return
      monarchRevision = revision
      setMonarch(holder)
    }

    let timerSync: number | undefined

    let lastTimer: GameTimerState | null = null

    function receiveTimer(state: GameTimerState) {
      lastTimer = state
      setTimer({ state, receivedAt: performance.now() })
    }

    function attachDataChannel(peerId: string, dataChannel: RTCDataChannel) {
      const peer = peersRef.current.get(peerId)
      if (!peer) return
      peer.channel = dataChannel
      dataChannel.onmessage = (event) => handleData(peerId, event)
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
      const visible =
        !spectatorRef.current && canViewBoard(peerIdRef.current, remotePeerId, revealToRef.current)
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
        // Negotiate a video sender before admission, without asking spectators for
        // camera permission. A seated client's real camera replaces this track.
        const placeholder = document.createElement("canvas")
        placeholder.width = 1920
        placeholder.height = 1080
        placeholder.getContext("2d")?.fillRect(0, 0, 1920, 1080)
        const media = placeholder.captureStream(1)
        if (disposed) return media.getTracks().forEach((track) => track.stop())
        media.getVideoTracks().forEach((track) => {
          track.enabled = !cameraOffRef.current
        })
        localStreamRef.current = media
        setLocalStream(media)
        const captureVideo = document.createElement("video")
        captureVideo.muted = true
        captureVideo.playsInline = true
        captureVideo.srcObject = media
        void captureVideo.play().catch(() => {})
        localVideoRef.current = captureVideo

        socket = new Socket("/socket", { params: () => ({ token: config.socket_token }) })
        socket.onError(() => {
          setStatus("Reconnecting… Your game is saved.")
          // Socket tokens expire after a day; refresh from the still-authenticated
          // cookie session so the next automatic retry does not reuse an expired token.
          void api<{ data: TableConfig }>("/api/webcam-table/config")
            .then(({ data }) => {
              config.socket_token = data.socket_token
            })
            .catch(() => {})
        })
        socket.connect()
        const room = socket.channel(`webcam_table:${roomId}`, () => ({
          protocol: 2,
          peer_id: peerIdRef.current,
          player_id: playerId,
          deck_id: deckId,
        }))
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
        room.on(
          "seat_order",
          ({ peer_ids, shuffled }: { peer_ids: string[]; shuffled: boolean }) => {
            setSeatOrder(peer_ids)
            if (shuffled) setShuffleVersion((version) => version + 1)
            log([
              shuffled
                ? "Seat order randomized"
                : lastTimer?.started_at == null
                  ? "Game started in seat order"
                  : "Seat order changed",
            ])
          },
        )
        room.on("monarch_state", syncMonarch)
        room.on("monarch", (event: MonarchEvent) => {
          syncMonarch(event)
          const { holder } = event
          log([holder ? `${holder.player_name} took the monarch` : "The monarch left the table"])
        })
        room.on("deck_selected", () => {
          void queryClient.invalidateQueries({ queryKey: ["decks"] })
        })
        room.on("seat_replaced", () => {
          setError("This seat is now open in another tab. Close this tab to keep playing there.")
          room.leave()
          socket?.disconnect()
        })
        room.onError(() => {
          // A channel retry is a new media generation. Reusing its peer ID can
          // leave one browser offering to an old connection after Presence resets.
          peerIdRef.current = crypto.randomUUID()
          for (const peer of peersRef.current.values()) {
            peer.connection.onconnectionstatechange = null
            peer.videoTrack.stop()
            peer.connection.close()
          }
          peersRef.current.clear()
          setStreams({})
          setConnectionStates({})
          setStatus("Reconnecting… Your game is saved.")
        })
        // The server starts every game with empty boards and says so in table_state.
        room.on("identified_cards", ({ entries }: { entries: BoardCard[] }) =>
          receiveCards(entries),
        )
        room.on(
          "table_state",
          ({
            timer: state,
            peer_ids,
            eliminated_seats,
            turns: turnState,
            mode: gameMode = "commander",
            team_life = {},
            seats,
            owner_id,
            monarch: savedMonarch,
            cards,
          }: {
            timer: GameTimerState
            peer_ids: string[]
            eliminated_seats: TableParticipant[]
            turns: TurnState
            mode?: GameFormat
            team_life?: Record<number, number>
            seats?: TableParticipant[]
            owner_id?: number
            monarch?: MonarchEvent
            cards?: BoardCard[]
          }) => {
            receiveTimer(state)
            setSeatOrder(peer_ids)
            setEliminatedSeats(seats ?? eliminated_seats)
            setTurns(turnState)
            setModeState(gameMode)
            setTeamLife(team_life)
            if (owner_id !== undefined) setOwnerId(owner_id)
            if (savedMonarch) syncMonarch(savedMonarch)
            if (cards) receiveCards(cards)
          },
        )
        room.on(
          "eliminated_seats",
          ({ participants: eliminated }: { participants: TableParticipant[] }) =>
            setEliminatedSeats(eliminated),
        )
        room.on("timer_state", receiveTimer)
        room.on("roll", (result: TableRoll) => {
          setRoll(result)
          const prefix =
            result.kind === "dice"
              ? `${result.player_name} rolled a d${result.sides}: `
              : `${result.player_name} flipped a coin: `
          log([
            {
              text: describeRoll(result),
              actor: result.actor,
              kind: result.kind === "dice" ? `dice:${result.sides}` : "coin",
              roll: { prefix, results: [result.result] },
            },
          ])
        })
        // Re-anchor to server time so wall-clock changes and browser clock drift cannot accumulate.
        const syncTimer = () => {
          if (room.state !== "joined") return
          const sentAt = performance.now()
          room.push("timer_sync", {}).receive("ok", (state: GameTimerState) => {
            if (disposed) return
            setTimer({ state, receivedAt: (sentAt + performance.now()) / 2 })
          })
        }
        timerSync = window.setInterval(syncTimer, 15_000)
        presence.onSync(() => {
          const next = presence?.list((_id, value) => value.metas[0] as TableParticipant) ?? []
          const seats = next.filter((participant) => !participant.spectator)
          participantsRef.current = seats
          setParticipants(seats)
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
            if (spectatorRef.current && participant.spectator) continue
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
          .receive("ok", ({ participant }: { participant?: TableParticipant }) => {
            monarchRevision = -1
            setError(null)
            spectatorRef.current = participant?.spectator ?? false
            setSpectating(spectatorRef.current)
            if (participant) {
              lifeRef.current = participant.life
              setLifeState(participant.life)
              const restored = {
                poison: participant.poison,
                rad: participant.rad,
                commander_casts: participant.commander_casts,
                commander_damage: participant.commander_damage,
              }
              countersRef.current = restored
              setCounters(restored)
              revealToRef.current = participant.reveal_to ?? null
              setRevealTo(revealToRef.current)
            }
            setStatus(
              spectatorRef.current
                ? "Spectating — this game has already started"
                : "Live — click any board to inspect a card",
            )
            syncTimer()
            if (!spectatorRef.current) {
              room.push("update_status", { camera_off: cameraOffRef.current })
              if (!cameraStarted) {
                cameraStarted = true
                void changeCamera(deviceIdRef.current)
              }
            }
            refreshVideo()
          })
          .receive("error", ({ reason }: { reason: string }) => setError(reason))
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : "Could not start the webcam table")
      }
    }

    void run()
    return () => {
      disposed = true
      cameraRequest.current += 1
      window.clearInterval(timerSync)
      channelRef.current?.leave()
      socket?.disconnect()
      peersRef.current.forEach((peer) => {
        peer.videoTrack.stop()
        peer.connection.close()
      })
      peersRef.current.clear()
      localStreamRef.current?.getTracks().forEach((track) => track.stop())
    }
  }, [deckId, handleData, log, playerId, queryClient, receiveCards, refreshVideo, roomId])

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
    if (spectatorRef.current || channelRef.current?.state !== "joined") return
    if (mode === "two_headed_giant") {
      const index = seatedParticipants.findIndex((seat) => seat.player_id === playerId)
      if (index >= 0) adjustTeamLife(Math.floor(index / 2), delta)
      return
    }
    const next = Math.max(-999, Math.min(999, lifeRef.current + delta))
    lifeRef.current = next
    setLifeState(next)
    updateStatus({ life: next })
  }

  function adjustCounter(counter: Counter, delta: number) {
    if (spectatorRef.current || channelRef.current?.state !== "joined") return
    const next = changeCounter(countersRef.current, counter, delta)
    countersRef.current = next
    setCounters(next)
    updateStatus(next)
  }

  function takeMonarch() {
    channelRef.current?.push("take_monarch", {})
  }

  function toggleCamera() {
    const next = !cameraOffRef.current
    cameraOffRef.current = next
    localStreamRef.current?.getVideoTracks().forEach((track) => {
      track.enabled = !next
    })
    refreshVideo()
    setCameraOff(next)
    updateStatus({ camera_off: next })
  }

  /** Owner starts the match, either keeping the arranged order or shuffling it. */
  function startGame(randomize: boolean) {
    channelRef.current
      ?.push("start_game", { randomize })
      .receive("ok", () => setError(null))
      .receive("error", ({ reason }: { reason: string }) => setError(reason))
  }

  const passTurn = useCallback(() => {
    channelRef.current
      ?.push("pass_turn", { revision: turns.revision })
      .receive("error", ({ reason }: { reason: string }) => setError(reason))
  }, [turns.revision])

  function adjustTurn(playerId: number, delta: -1 | 1) {
    channelRef.current
      ?.push("adjust_turn", { player_id: playerId, delta })
      .receive("error", ({ reason }: { reason: string }) => setError(reason))
  }

  function setMode(mode: GameFormat) {
    channelRef.current
      ?.push("set_mode", { mode })
      .receive("ok", () => setError(null))
      .receive("error", ({ reason }: { reason: string }) => setError(reason))
  }

  function adjustTeamLife(teamIndex: number, delta: number) {
    channelRef.current
      ?.push("adjust_team_life", { team_index: teamIndex, delta })
      .receive("error", ({ reason }: { reason: string }) => setError(reason))
  }

  /**
   * Owner swaps a seat with its neighbour. Before the match starts this only
   * rearranges; once started (Commander only) the server re-seats mid-game.
   */
  function moveSeat(peerId: string, delta: -1 | 1) {
    const peers = seatedParticipants.map((seat) => seat.peer_id)
    const index = peers.indexOf(peerId)
    const other = index + delta
    if (index < 0 || other < 0 || other >= peers.length) return
    ;[peers[index], peers[other]] = [peers[other]!, peers[index]!]
    channelRef.current
      ?.push(timer?.state.started_at == null ? "arrange_seats" : "seat_order", { peer_ids: peers })
      .receive("error", ({ reason }: { reason: string }) => setError(reason))
  }

  function changeTimer(action: "pause" | "resume"): Promise<GameTimerState | null> {
    return new Promise((resolve) => {
      const channel = channelRef.current
      if (channel?.state !== "joined") {
        setError("Reconnect to the table before changing the timer")
        resolve(null)
        return
      }
      channel
        .push("timer", { action })
        .receive("ok", (state: GameTimerState) => {
          setTimer({ state, receivedAt: performance.now() })
          resolve(state)
        })
        .receive("error", ({ reason }: { reason: string }) => {
          setError(reason)
          resolve(null)
        })
        .receive("timeout", () => {
          setError("Timer request timed out; try again")
          resolve(null)
        })
    })
  }

  function setEliminated(peerId: string, eliminated: boolean) {
    channelRef.current
      ?.push("set_eliminated", { peer_id: peerId, eliminated })
      .receive("error", ({ reason }: { reason: string }) => setError(reason))
      .receive("timeout", () => setError("Elimination request timed out; try again"))
  }

  function rollDice(request: RollRequest) {
    channelRef.current
      ?.push("roll", request)
      .receive("error", ({ reason }: { reason: string }) => setError(reason))
      .receive("timeout", () =>
        setError("Roll request timed out; check the table log before retrying"),
      )
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
    sendCardCommand({ type: "card_identified", entry })
    return entry
  }

  /** Takes a misidentified card off its board's list at every seat. */
  function removeCard(id: string) {
    sendCardCommand({ type: "card_removed", id })
  }

  /** Empties the local seat's own board at every seat; other boards are not ours to clear. */
  function clearOwnCards() {
    sendCardCommand({ type: "cards_cleared", ownerPeerId: peerIdRef.current })
  }

  /** Participants in shared seat order; the End game form records seats in this order. */
  const seatedParticipants = useMemo(
    () =>
      orderBySeats(retainEliminatedSeats(participants, eliminatedSeats), seatOrder).map(
        (liveParticipant) => {
          // Durable table state owns elimination, including offline teammates.
          const saved = eliminatedSeats.find((seat) => seat.player_id === liveParticipant.player_id)
          const participant = saved
            ? { ...liveParticipant, eliminated: saved.eliminated }
            : liveParticipant
          return participant.peer_id === peerIdRef.current
            ? { ...participant, life, ...counters }
            : participant
        },
      ),
    [counters, life, participants, eliminatedSeats, seatOrder],
  )

  return {
    spectating,
    isOwner: ownerId === playerId,
    peerId: peerIdRef.current,
    participants: seatedParticipants,
    setEliminated,
    shuffleVersion,
    timer,
    turns,
    mode,
    setMode,
    teamLife,
    adjustTeamLife,
    moveSeat,
    passTurn,
    adjustTurn,
    roll,
    changeTimer,
    rollDice,
    events,
    streams,
    connectionStates,
    iceServers,
    localStream,
    changeCamera,
    cameraChanging,
    cameraError,
    getPeerStats,
    cameraOff,
    revealTo,
    revealBusy,
    changeReveal,
    capture,
    identifiedCards,
    status,
    error,
    requestCapture,
    announceCard,
    removeCard,
    clearOwnCards,
    chooseDeck,
    life,
    changeLife,
    counters,
    adjustCounter,
    monarch,
    takeMonarch,
    toggleCamera,
    startGame,
    dismissCapture: () => setCapture(null),
  }
}
