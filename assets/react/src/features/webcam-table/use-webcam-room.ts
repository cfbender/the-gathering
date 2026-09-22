import { Channel, Presence, Socket } from "phoenix"
import { useCallback, useEffect, useRef, useState } from "react"
import { api } from "@/lib/api"

export interface TableParticipant {
  peer_id: string
  player_id: number
  player_name: string
  deck_id?: number
  deck_name?: string
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
  image: string
  nativeWidth: number
  nativeHeight: number
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
    }
  | { type: "deck_suggestion"; deckId: number }

interface PeerState {
  connection: RTCPeerConnection
  channel?: RTCDataChannel
  stream?: MediaStream
  candidates: RTCIceCandidateInit[]
}

const CROP_SIZE = 640

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
  return { image: canvas.toDataURL("image/jpeg", 0.82), nativeWidth: width, nativeHeight: height }
}

export function useWebcamRoom(roomId: string, playerId: number, deckId: number | null) {
  const peerIdRef = useRef(crypto.randomUUID())
  const localVideoRef = useRef<HTMLVideoElement | null>(null)
  const localStreamRef = useRef<MediaStream | null>(null)
  const channelRef = useRef<Channel | null>(null)
  const peersRef = useRef(new Map<string, PeerState>())
  const participantsRef = useRef<TableParticipant[]>([])
  const pendingCaptures = useRef(new Map<string, string>())
  const [participants, setParticipants] = useState<TableParticipant[]>([])
  const [streams, setStreams] = useState<Record<string, MediaStream>>({})
  const [localStream, setLocalStream] = useState<MediaStream | null>(null)
  const [capture, setCapture] = useState<CapturedCard | null>(null)
  const [status, setStatus] = useState("Opening 1080p camera…")
  const [error, setError] = useState<string | null>(null)

  const chooseDeck = useCallback((chosenDeckId: number) => {
    channelRef.current?.push("choose_deck", { deck_id: chosenDeckId })
  }, [])

  const handleData = useCallback(
    (fromPeerId: string, event: MessageEvent<string>) => {
      const message = JSON.parse(event.data) as DataMessage
      if (message.type === "capture_request" && localVideoRef.current) {
        const result = captureCrop(localVideoRef.current, message.x, message.y)
        peersRef.current
          .get(fromPeerId)
          ?.channel?.send(
            JSON.stringify({ type: "capture_response", requestId: message.requestId, ...result }),
          )
      } else if (message.type === "capture_response") {
        pendingCaptures.current.delete(message.requestId)
        const owner = participantsRef.current.find((item) => item.peer_id === fromPeerId)
        if (owner) setCapture({ peerId: fromPeerId, playerId: owner.player_id, ...message })
      } else if (message.type === "deck_suggestion") {
        chooseDeck(message.deckId)
      }
    },
    [chooseDeck],
  )

  useEffect(() => {
    let disposed = false
    let socket: Socket | null = null
    let presence: Presence | null = null

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
      const peer: PeerState = { connection, candidates: [] }
      peersRef.current.set(remotePeerId, peer)
      localStreamRef.current
        ?.getTracks()
        .forEach((track) => connection.addTrack(track, localStreamRef.current as MediaStream))
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
        if (["failed", "closed"].includes(connection.connectionState)) {
          setStreams((current) => {
            const next = { ...current }
            delete next[remotePeerId]
            return next
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
        presence.onSync(() => {
          const next = presence?.list((_id, value) => value.metas[0] as TableParticipant) ?? []
          participantsRef.current = next
          setParticipants(next)
          const activeIds = new Set(next.map((item) => item.peer_id))
          peersRef.current.forEach((peer, id) => {
            if (!activeIds.has(id)) {
              peer.connection.close()
              peersRef.current.delete(id)
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
          .receive("ok", () => setStatus("Live — click any board to inspect a card"))
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
      peersRef.current.forEach((peer) => peer.connection.close())
      peersRef.current.clear()
      localStreamRef.current?.getTracks().forEach((track) => track.stop())
    }
  }, [deckId, handleData, playerId, roomId])

  function requestCapture(targetPeerId: string, x: number, y: number) {
    if (targetPeerId === peerIdRef.current && localVideoRef.current) {
      const result = captureCrop(localVideoRef.current, x, y)
      setCapture({ peerId: targetPeerId, playerId, ...result })
      return
    }
    const requestId = crypto.randomUUID()
    pendingCaptures.current.set(requestId, targetPeerId)
    peersRef.current
      .get(targetPeerId)
      ?.channel?.send(JSON.stringify({ type: "capture_request", requestId, x, y }))
    setStatus("Requesting native camera crop…")
  }

  function suggestDeck(targetPeerId: string, suggestedDeckId: number) {
    if (targetPeerId === peerIdRef.current) chooseDeck(suggestedDeckId)
    else
      peersRef.current
        .get(targetPeerId)
        ?.channel?.send(JSON.stringify({ type: "deck_suggestion", deckId: suggestedDeckId }))
    setCapture(null)
  }

  return {
    peerId: peerIdRef.current,
    participants,
    streams,
    localStream,
    capture,
    status,
    error,
    requestCapture,
    suggestDeck,
    chooseDeck,
    dismissCapture: () => setCapture(null),
  }
}
