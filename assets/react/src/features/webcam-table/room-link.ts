import type { Channel } from "phoenix"
import { useRef } from "react"
import type { TableParticipant } from "./room-types"

/** What every table hook needs to know about this tab's membership at event time, without
 * re-rendering on it. One stable object per room hook; each field has a single writer. */
export interface RoomLink {
  /** This tab's media generation. `useRoomChannel` replaces it when the channel rejoins. */
  peerId: string
  /** Set by `useRoomChannel` once the socket opens the room channel. */
  channel: Channel | null
  /** Set by `useRoomChannel` from the join reply. */
  spectator: boolean
  /** Seated participants from the last presence sync; written by `useTableGameState`. */
  participants: TableParticipant[]
}

export function useRoomLink(): RoomLink {
  const link = useRef<RoomLink | null>(null)
  link.current ??= {
    peerId: crypto.randomUUID(),
    channel: null,
    spectator: false,
    participants: [],
  }
  return link.current
}

export function liveStatus(spectator: boolean) {
  return spectator
    ? "Spectating — this game has already started"
    : "Live — click any board to inspect a card"
}
