import type { SelectedCard } from "@/lib/cards"

export interface DraftSeat {
  id?: number
  playerName: string
  deckName: string
  commander: SelectedCard | null
  partner: SelectedCard | null
  colorIdentity: string
  decklistUrl: string
  mvpCard: SelectedCard | null
}

export const blankSeat = (): DraftSeat => ({
  playerName: "",
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

export function resultsForSeats(count: number, winnerIndex: number | null) {
  return Array.from({ length: count }, (_, index) =>
    winnerIndex === null ? "draw" : index === winnerIndex ? "win" : "loss",
  ) as Array<"draw" | "win" | "loss">
}
