import type { TableParticipant } from "./use-webcam-room"

/** A table log entry as the server sends it; `at` is milliseconds since the epoch. */
export interface TableLogEntry {
  id: number
  at: number
  text: string
  count?: number
}

export interface TableEvent {
  id: number
  at: Date
  text: string
  count?: number
}

export function toTableEvent({ id, at, text, count }: TableLogEntry): TableEvent {
  return { id, at: new Date(at), text, count }
}

/** Newest first. A merge re-sends the head entry under its id; anything else is new. */
export function receiveTableEvent(events: TableEvent[], next: TableEvent): TableEvent[] {
  if (events.some((event) => event.id === next.id))
    return events.map((event) => (event.id === next.id ? next : event))
  return [next, ...events].slice(0, 200)
}

/** Playing order skips out seats; recording order still includes every seat. */
export function activeTurnOrder(participants: TableParticipant[]): TableParticipant[] {
  return participants.filter((participant) => !participant.eliminated)
}

/** Eliminated players remain recordable after leaving; a rejoin replaces the retained seat. */
export function retainEliminatedSeats(
  live: TableParticipant[],
  eliminated: TableParticipant[],
): TableParticipant[] {
  const present = new Set(live.map((participant) => participant.player_id))
  return [
    ...live,
    ...eliminated
      .filter((participant) => !present.has(participant.player_id))
      .map((participant) => ({ ...participant, departed: true })),
  ]
}

/** Random seat order that, for two or more seats, differs from the current one so a
 * "Randomize" click always visibly does something. */
export function shuffleSeats<T>(items: T[], random: () => number = Math.random): T[] {
  let next: T[] = [...items]
  for (let attempt = 0; attempt < 16; attempt += 1) {
    next = []
    const pool = [...items]
    while (pool.length > 0) {
      const [picked] = pool.splice(Math.floor(random() * pool.length), 1)
      if (picked !== undefined) next.push(picked)
    }
    if (items.length < 2 || next.some((item, index) => item !== items[index])) break
  }
  return next
}

/** Orders participants by the shared seat order, then anyone the order does not name yet in
 * join order (server-stamped, so every browser agrees). */
export function orderBySeats(
  participants: TableParticipant[],
  seatOrder: string[],
): TableParticipant[] {
  const byPeer = new Map(participants.map((participant) => [participant.peer_id, participant]))
  const seated = seatOrder.flatMap((peerId) => {
    const participant = byPeer.get(peerId)
    if (!participant) return []
    byPeer.delete(peerId)
    return [participant]
  })
  const unseated = [...byPeer.values()].sort(
    (a, b) => a.joined_at - b.joined_at || a.peer_id.localeCompare(b.peer_id),
  )
  return [...seated, ...unseated]
}
