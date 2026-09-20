import type { SelectedCard } from "@/lib/cards"

export interface DraftSeat {
  draftId: string
  id?: number
  playerId: number | null
  playerName: string
  deckId: number | null
  deckName: string
  commander: SelectedCard | null
  partner: SelectedCard | null
  colorIdentity: string
  decklistUrl: string
  mvpCard: SelectedCard | null
}

let nextDraftSeatId = 0

export const blankSeat = (): DraftSeat => ({
  draftId: `new-seat-${++nextDraftSeatId}`,
  playerId: null,
  playerName: "",
  deckId: null,
  deckName: "",
  commander: null,
  partner: null,
  colorIdentity: "",
  decklistUrl: "",
  mvpCard: null,
})

export function moveSeat(seats: DraftSeat[], index: number, direction: -1 | 1) {
  const destination = index + direction
  if (destination < 0 || destination >= seats.length) return seats
  const next = [...seats]
  const current = next[index]
  const target = next[destination]
  if (!current || !target) return seats
  next[index] = target
  next[destination] = current
  return next
}

export function resultsForSeats(seats: DraftSeat[], winnerSeatId: string | null) {
  return seats.map((seat) =>
    winnerSeatId === null ? "draw" : seat.draftId === winnerSeatId ? "win" : "loss",
  ) as Array<"draw" | "win" | "loss">
}
