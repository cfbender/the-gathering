import type { GameFormat } from "@/features/games/game-format"
import type { TableParticipant } from "./use-webcam-room"

export function teams(seats: TableParticipant[]): TableParticipant[][] {
  return Array.from({ length: Math.ceil(seats.length / 2) }, (_, i) =>
    seats.slice(i * 2, i * 2 + 2),
  )
}

export function turnId(seats: TableParticipant[], playerId: number | null, mode: GameFormat) {
  if (mode !== "two_headed_giant") return playerId
  return (
    teams(seats).find((team) => team.some((seat) => seat.player_id === playerId))?.[0]?.player_id ??
    playerId
  )
}

/** Original seats define neighbours, even after an elimination or disconnect. */
export function unattackableSeats(seats: TableParticipant[], viewerId: number): string[] {
  const index = seats.findIndex((seat) => seat.player_id === viewerId)
  if (seats.length !== 5 || index < 0) return []
  const at = (offset: number) => seats[(index + offset) % 5]!
  if (at(2).eliminated && at(3).eliminated) return []
  return [at(4).peer_id, at(1).peer_id]
}
