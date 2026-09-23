import type { TableParticipant } from "./use-webcam-room"

/** Human-readable log lines for presence changes, in the order they matter to players. */
export function describeParticipantChange(
  previous: TableParticipant | undefined,
  next: TableParticipant,
): string[] {
  if (!previous) return [`${next.player_name} joined the table`]

  const lines: string[] = []
  if (next.deck_id !== previous.deck_id && next.deck_name) {
    lines.push(`${next.player_name} chose ${next.deck_name}`)
  }
  if (next.life !== previous.life) {
    lines.push(`${next.player_name} ${previous.life} → ${next.life} life`)
  }
  if (next.camera_off !== previous.camera_off) {
    lines.push(`${next.player_name} turned their camera ${next.camera_off ? "off" : "on"}`)
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
