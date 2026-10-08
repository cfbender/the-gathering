import type { TableChannel } from "./table-channel"
import { useCallback, useRef, useState } from "react"
import type { RoomLink } from "./room-link"
import {
  EMPTY_TRACKERS,
  adjustCounter,
  parseSavedTrackers,
  sharedTrackers,
  type SeatTrackers,
} from "./trackers"

function storageKey(roomId: string, playerId: number) {
  return `the-gathering:table-trackers:${roomId}:${playerId}`
}

function load(key: string): SeatTrackers {
  try {
    return parseSavedTrackers(JSON.parse(localStorage.getItem(key) ?? "null"))
  } catch {
    return EMPTY_TRACKERS
  }
}

/**
 * This seat's custom counters and combat buffs. The browser is their home: every edit is saved
 * per room so a refresh keeps them, and only the shared subset is published, through the same
 * `update_status` path as life. A rematch resets them along with the seat.
 */
export function useSeatTrackers(link: RoomLink, roomId: string, playerId: number) {
  const key = storageKey(roomId, playerId)
  const [trackers, setTrackersState] = useState(() => load(key))
  const trackersRef = useRef(trackers)

  const publish = useCallback(
    (next: SeatTrackers) => {
      if (link.spectator || link.channel?.state !== "joined") return
      link.channel.push("update_status", sharedTrackers(next))
    },
    [link],
  )

  const setTrackers = useCallback(
    (update: SeatTrackers | ((current: SeatTrackers) => SeatTrackers)) => {
      const previous = trackersRef.current
      const next = typeof update === "function" ? update(previous) : update
      if (next === previous) return
      trackersRef.current = next
      setTrackersState(next)
      try {
        if (next === EMPTY_TRACKERS) localStorage.removeItem(key)
        else localStorage.setItem(key, JSON.stringify(next))
      } catch {
        // Private browsing or a full quota: the trackers still work for this page load.
      }
      // Private edits never leave the browser.
      if (JSON.stringify(sharedTrackers(next)) !== JSON.stringify(sharedTrackers(previous)))
        publish(next)
    },
    [key, publish],
  )

  const bindChannel = useCallback(
    (room: TableChannel) => {
      room.on("seat_reset", () => setTrackers(EMPTY_TRACKERS))
    },
    [setTrackers],
  )

  return {
    trackers,
    setTrackers,
    adjustCounter: (id: string, delta: number) =>
      setTrackers((current) => adjustCounter(current, id, delta)),
    bindChannel,
    /** Re-sends the shared subset after a (re)join so the room has this browser's copy. */
    publish: () => publish(trackersRef.current),
  }
}
