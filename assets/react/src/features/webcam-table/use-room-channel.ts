import { type Channel, Presence, Socket } from "phoenix"
import { useEffect, useRef, useState } from "react"
import { api } from "@/lib/api"
import { liveStatus, type RoomLink } from "./room-link"
import type { TableParticipant } from "./room-types"
import type { Signal } from "./use-peer-connections"

/** Minimum gap between socket-token refreshes while reconnecting. The config endpoint allows
 * 20 requests per 5 minutes per account, and the page-load fetch is not counted here. */
export const TOKEN_REFRESH_INTERVAL_MS = 30_000

interface TableConfig {
  ice_servers: RTCIceServer[]
  max_players: number
  minimum_height: number
  socket_token: string
}

/** What the rest of the room does with the channel's lifecycle. Read at event time, so the
 * callbacks may change between renders without reconnecting. */
export interface RoomChannelHandlers {
  setStatus: (status: string) => void
  setError: (error: string | null) => void
  /** The table config arrived; the channel opens right after this returns. */
  onConfig: (iceServers: RTCIceServer[]) => void
  /** Register channel and presence bindings; runs before the first join. */
  bind: (room: Channel, presence: Presence) => void
  /** Everyone present after a presence sync, spectators included. */
  onPresence: (everyone: TableParticipant[]) => void
  onSignal: (from: string, signal: Signal) => void
  /** Every successful (re)join, after `link.spectator` is set. `owner` is the server's
   * decision that this seat holds the table controls (room creator or an admin). */
  onJoined: (participant: TableParticipant | undefined, owner: boolean) => void
  /** The channel dropped; `link.peerId` is already a new media generation. */
  onChannelError: () => void
  onDispose: () => void
}

interface JoinReply {
  participant?: TableParticipant
  owner?: boolean
}

/** The Phoenix socket, `webcam_table:<roomId>` channel, and presence for one seat. */
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

  useEffect(() => {
    let disposed = false
    let socket: Socket | null = null
    const on = () => handlersRef.current

    async function run() {
      try {
        const config = await api<{ data: TableConfig }>("/api/webcam-table/config").then(
          (body) => body.data,
        )
        if (disposed) return
        on().onConfig(config.ice_servers)

        socket = new Socket("/socket", { params: () => ({ token: config.socket_token }) })
        let refreshing = false
        let refreshedAt: number | null = null
        socket.onError(() => {
          on().setStatus("Reconnecting… Your game is saved.")
          // Socket tokens expire after a day; refresh from the still-authenticated
          // cookie session so the next automatic retry does not reuse an expired token.
          // phoenix.js retries every few seconds while the server is away, and the config
          // endpoint is rate-limited (it mints TURN credentials), so refresh at most once
          // per interval instead of on every failed attempt.
          const now = Date.now()
          if (refreshing || (refreshedAt !== null && now - refreshedAt < TOKEN_REFRESH_INTERVAL_MS))
            return
          refreshing = true
          refreshedAt = now
          void api<{ data: TableConfig }>("/api/webcam-table/config")
            .then(({ data }) => {
              config.socket_token = data.socket_token
            })
            .catch(() => {})
            .finally(() => {
              refreshing = false
            })
        })
        socket.connect()
        const room = socket.channel(`webcam_table:${roomId}`, () => ({
          peer_id: link.peerId,
          player_id: playerId,
          deck_id: deckId,
        }))
        link.channel = room
        const presence = new Presence(room)
        on().bind(room, presence)
        presence.onSync(() =>
          on().onPresence(presence.list((_id, value) => value.metas[0] as TableParticipant)),
        )
        room.on(
          "signal",
          ({ target, from, signal }: { target: string; from: string; signal: Signal }) => {
            if (target === link.peerId) on().onSignal(from, signal)
          },
        )
        room.on("seat_replaced", () => {
          on().setError(
            "This seat is now open in another tab. Close this tab to keep playing there.",
          )
          room.leave()
          socket?.disconnect()
        })
        room.onError(() => {
          // A channel retry is a new media generation. Reusing its peer ID can
          // leave one browser offering to an old connection after Presence resets.
          link.peerId = crypto.randomUUID()
          on().onChannelError()
          on().setStatus("Reconnecting… Your game is saved.")
        })
        room
          .join()
          .receive("ok", ({ participant, owner = false }: JoinReply) => {
            on().setError(null)
            link.spectator = participant?.spectator ?? false
            setSpectating(link.spectator)
            on().setStatus(liveStatus(link.spectator))
            on().onJoined(participant, owner)
          })
          .receive("error", ({ reason }: { reason: string }) => on().setError(reason))
      } catch (reason) {
        on().setError(reason instanceof Error ? reason.message : "Could not start the webcam table")
      }
    }

    void run()
    return () => {
      disposed = true
      link.channel?.leave()
      link.channel = null
      socket?.disconnect()
      handlersRef.current.onDispose()
    }
  }, [deckId, link, playerId, roomId])

  return { spectating }
}
