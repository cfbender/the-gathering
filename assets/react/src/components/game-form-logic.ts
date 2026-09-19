export interface DraftSeat {
  id?: number
  playerName: string
  deckName: string
  commanderName: string
  mvpCardName: string
}

export const blankSeat = (): DraftSeat => ({
  playerName: "",
  deckName: "",
  commanderName: "",
  mvpCardName: "",
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
