import { useEffect, useRef, useState } from "react"
import { io, type Socket } from "socket.io-client"
import { api } from "@/lib/api"
import { liveStatus, type RoomLink } from "./room-link"
import type { TableParticipant } from "./room-types"
import { TableChannel } from "./table-channel"
import type { SfuOffer } from "./use-sfu-connection"

/** Minimum gap between socket-token refreshes while reconnecting. The config endpoint allows
 * 20 requests per 5 minutes per account, and the page-load fetch is not counted here. */
export const TOKEN_REFRESH_INTERVAL_MS = 30_000

export interface SfuInfo {
  /** Whether media reaches the server directly (forwarded UDP ports) or through TURN. */
  transport: "direct" | "relay"
}

interface TableConfig {
  ice_servers: RTCIceServer[]
  max_players: number
  minimum_height: number
  socket_token: string
  sfu: SfuInfo
}

/** What the rest of the room does with the channel's lifecycle. Read at event time, so the
 * callbacks may change between renders without reconnecting. */
export interface RoomChannelHandlers {
  setStatus: (status: string) => void
  setError: (error: string | null) => void
  /** The table config arrived; the channel opens right after this returns. */
  onConfig: (iceServers: RTCIceServer[], sfu: SfuInfo) => void
  /** Register channel bindings; runs before the first join. */
  bind: (room: TableChannel) => void
  /** Everyone present whenever the roster changes, spectators included. */
  onPresence: (everyone: TableParticipant[]) => void
  /** The server offers this seat a new set of boards (or asks to drop one). */
  onSfuOffer: (offer: SfuOffer) => void
  onSfuCandidate: (payload: { candidate: RTCIceCandidateInit }) => void
  /** A message from another seat, relayed by the server. */
  onPeerMessage: (payload: { from: string; message: unknown }) => void
  /** Every successful (re)join, after `link.spectator` is set. `owner` is the server's
   * decision that this seat holds the table controls (room creator or an admin). */
  onJoined: (participant: TableParticipant | undefined, owner: boolean) => void
  /** The seat is about to rejoin after a drop; `link.peerId` is already a new media
   * generation. Runs when the rejoin is sent, not when the connection drops, so a brief drop
   * leaves the table on screen as it was. */
  onChannelError: () => void
  /** The room owner ended the table; the channel and socket are already closed. */
  onClosed: () => void
  onRemoved: () => void
  onDispose: () => void
}

interface JoinReply {
  participant?: TableParticipant
  owner?: boolean
  can_moderate?: boolean
}

/** The table socket, this seat's channel, and the presence roster for one seat. */
export function useRoomChannel(
  link: RoomLink,
  roomId: string,
  playerId: number,
  deckId: number | null,
  handlers: RoomChannelHandlers,
) {
  const handlersRef = useRef(handlers)
  handlersRef.current = handlers
  const [spectating, setSpectating] = useState(false)
  const [canModerate, setCanModerate] = useState(false)

  useEffect(() => {
    let disposed = false
    let socket: Socket | null = null
    let retryTimer: ReturnType<typeof setTimeout> | undefined
    const on = () => handlersRef.current

    async function run() {
      try {
        const config = await api<{ data: TableConfig }>("/api/webcam-table/config").then(
          (body) => body.data,
        )
        if (disposed) return
        on().onConfig(config.ice_servers, config.sfu)

        socket = io({
          transports: ["websocket"],
          // Read on every connection attempt, so reconnects use a refreshed token.
          auth: (send) => send({ token: config.socket_token }),
        })
        const current = socket
        let refreshing: Promise<void> | null = null
        let refreshedAt: number | null = null
        // Socket tokens expire after a day; refresh from the still-authenticated cookie session
        // so the next attempt does not reuse an expired token. Socket.IO retries every few
        // seconds while the server is away, and the config endpoint is rate-limited (it mints
        // TURN credentials), so refresh at most once per interval instead of on every attempt.
        const refreshToken = () => {
          const now = Date.now()
          if (refreshing || (refreshedAt !== null && now - refreshedAt < TOKEN_REFRESH_INTERVAL_MS))
            return refreshing
          refreshedAt = now
          refreshing = api<{ data: TableConfig }>("/api/webcam-table/config")
            .then(({ data }) => {
              config.socket_token = data.socket_token
            })
            .catch(() => {})
            .finally(() => {
              refreshing = null
            })
          return refreshing
        }
        // Socket.IO does not retry a connection the server refused or closed, so retry with a
        // fresh token once the refresh interval allows.
        const reconnect = () => {
          clearTimeout(retryTimer)
          const wait =
            refreshedAt === null
              ? 0
              : Math.max(0, refreshedAt + TOKEN_REFRESH_INTERVAL_MS - Date.now())
          retryTimer = setTimeout(() => {
            void Promise.resolve(refreshToken()).then(() => {
              if (!disposed) current.connect()
            })
          }, wait)
        }
        socket.on("disconnect", (reason, details) => {
          // A disconnect is routine on leaving; a stream of them is why seats keep "Connecting…".
          console.warn("Table socket disconnected", reason, details)
          if (reason === "io server disconnect") {
            on().setStatus("Reconnecting… Your game is saved.")
            reconnect()
          }
        })
        socket.on("connect_error", (error) => {
          console.warn(
            "Table socket error",
            error.message,
            (error as { description?: unknown }).description,
          )
          on().setStatus("Reconnecting… Your game is saved.")
          if (current.active) void refreshToken()
          else reconnect()
        })
        // Set when the seat drops. The next join starts a new media generation: a retry that
        // reused the peer ID could leave this browser offering to the old connection.
        let dropped = false
        const room = new TableChannel(socket, () => {
          if (dropped) {
            dropped = false
            link.peerId = crypto.randomUUID()
            on().onChannelError()
          }
          return { room_id: roomId, peer_id: link.peerId, player_id: playerId, deck_id: deckId }
        })
        link.channel = room
        on().bind(room)
        // A player holds one seat. Until the server has replaced this tab's previous seat (a
        // connection that dropped without closing), the roster can still list it; never show it
        // as another player.
        room.on("presence", (everyone: TableParticipant[]) =>
          on().onPresence(
            everyone.filter(
              (participant) =>
                participant.player_id !== playerId || participant.peer_id === link.peerId,
            ),
          ),
        )
        room.on("sfu_offer", (offer: SfuOffer) => on().onSfuOffer(offer))
        room.on("sfu_candidate", (payload: { candidate: RTCIceCandidateInit }) =>
          on().onSfuCandidate(payload),
        )
        room.on("peer_message", (payload: { from: string; message: unknown }) =>
          on().onPeerMessage(payload),
        )
        // The owner ended the table; the server ends this seat's channel right after.
        room.on("table_closed", () => {
          room.leave()
          socket?.disconnect()
          on().onClosed()
        })
        const removed = () => {
          disposed = true
          clearTimeout(retryTimer)
          room.leave()
          socket?.disconnect()
          setCanModerate(false)
          on().onRemoved()
        }
        room.on("removed", removed)
        room.on("seat_replaced", () => {
          on().setError(
            "This seat is now open in another tab. Close this tab to keep playing there.",
          )
          room.leave()
          socket?.disconnect()
        })
        room.onError((reason) => {
          console.warn("Table channel error; rejoining with a new seat connection", reason)
          dropped = true
          on().setStatus("Reconnecting… Your game is saved.")
        })
        room
          .join()
          .receive("ok", ({ participant, owner = false, can_moderate = false }: JoinReply) => {
            on().setError(null)
            setCanModerate(can_moderate)
            link.spectator = participant?.spectator ?? false
            setSpectating(link.spectator)
            on().setStatus(liveStatus(link.spectator))
            on().onJoined(participant, owner)
          })
          .receive("error", ({ reason }: { reason: string }) => {
            if (reason === "You were removed from this table.") removed()
            else on().setError(reason)
          })
      } catch (reason) {
        on().setError(reason instanceof Error ? reason.message : "Could not start the webcam table")
      }
    }

    void run()
    return () => {
      disposed = true
      clearTimeout(retryTimer)
      link.channel?.leave()
      link.channel = null
      socket?.disconnect()
      handlersRef.current.onDispose()
    }
  }, [deckId, link, playerId, roomId])

  return { spectating, canModerate }
}
