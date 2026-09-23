import type { TableParticipant } from "./use-webcam-room"

export interface TableEventContent {
  text: string
  actor?: string
  kind?: string
  life?: { name: string; from: number; to: number }
  roll?: { prefix: string; results: (number | string)[] }
}

export interface TableEvent extends TableEventContent {
  id: number
  at: Date
  count?: number
}

/** Newest first. Only adjacent events from the same actor/kind within two seconds merge. */
export function appendTableEvent(events: TableEvent[], next: TableEvent): TableEvent[] {
  const previous = events[0]
  const gap = previous ? next.at.getTime() - previous.at.getTime() : Infinity
  if (
    !previous ||
    !next.kind ||
    !next.actor ||
    previous.kind !== next.kind ||
    previous.actor !== next.actor ||
    gap < 0 ||
    gap > 2000
  ) {
    return [next, ...events].slice(0, 200)
  }
  const merged = { ...next, id: previous.id, count: (previous.count ?? 1) + 1 }
  if (previous.life && next.life) {
    merged.life = { ...next.life, from: previous.life.from }
    merged.text = `${next.life.name}: ${previous.life.from} → ${next.life.to} life`
  } else if (previous.roll && next.roll) {
    merged.roll = { ...next.roll, results: [...previous.roll.results, ...next.roll.results] }
    merged.text = `${next.roll.prefix}${merged.roll.results.join(", ")}`
  }
  return [merged, ...events.slice(1)]
}

/** Human-readable log lines for presence changes, in the order they matter to players. */
export function describeParticipantChange(
  previous: TableParticipant | undefined,
  next: TableParticipant,
): TableEventContent[] {
  if (!previous) return [{ text: `${next.player_name} joined the table` }]

  const lines: TableEventContent[] = []
  if (next.deck_id !== previous.deck_id && next.deck_name) {
    lines.push({
      text: `${next.player_name} chose ${next.deck_name}`,
      actor: next.peer_id,
      kind: "deck",
    })
  }
  if (next.life !== previous.life) {
    lines.push({
      text: `${next.player_name}: ${previous.life} → ${next.life} life`,
      actor: next.peer_id,
      kind: "life",
      life: { name: next.player_name, from: previous.life, to: next.life },
    })
  }
  if (next.camera_off !== previous.camera_off) {
    lines.push({
      text: `${next.player_name} turned their camera ${next.camera_off ? "off" : "on"}`,
      actor: next.peer_id,
      kind: "camera",
    })
  }
  return lines
}

export function describeParticipantLeft(participant: TableParticipant) {
  return `${participant.player_name} left the table`
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
