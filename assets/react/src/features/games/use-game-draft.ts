import type { SelectedCard } from "@/lib/cards"
import { cardSnapshot } from "@/lib/cards"
import { useState } from "react"
import type { Game } from "@/features/games/games"

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
  kills: string
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
  kills: "",
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

function localDateTime(value?: string) {
  const date = value ? new Date(value) : new Date()
  const offset = date.getTimezoneOffset() * 60_000
  return new Date(date.getTime() - offset).toISOString().slice(0, 16)
}

function draftsFromGame(game: Game): DraftSeat[] {
  return game.seats.map((seat) => ({
    draftId: `persisted-seat-${seat.id}`,
    id: seat.id,
    playerId: seat.player_id,
    playerName: seat.player.name,
    deckId: seat.deck_id,
    deckName: seat.deck?.name ?? "",
    commander: cardSnapshot(seat.deck?.commander_card_id, seat.deck?.commander_name),
    partner: cardSnapshot(seat.deck?.partner_card_id, seat.deck?.partner_name),
    colorIdentity: seat.deck?.color_identity ?? "",
    decklistUrl: seat.deck?.decklist_url ?? "",
    kills: seat.kills?.toString() ?? "",
    mvpCard: cardSnapshot(seat.mvp_card_id, seat.mvp_card_name),
  }))
}

/** Owns the editable state and stable seat identities for one game form. */
export function useGameDraft(game?: Game) {
  const [playedAt, setPlayedAt] = useState(() => localDateTime(game?.played_at))
  const [seats, setSeats] = useState<DraftSeat[]>(() =>
    game ? draftsFromGame(game) : [blankSeat(), blankSeat()],
  )
  const [winnerSeatId, setWinnerSeatId] = useState<string | null>(() => {
    if (!game) return seats[0]?.draftId ?? null
    const winner = game.seats.find((seat) => seat.result === "win")
    return winner ? `persisted-seat-${winner.id}` : null
  })
  const [turns, setTurns] = useState(game?.turns?.toString() ?? "")
  const [duration, setDuration] = useState(game?.duration_minutes?.toString() ?? "")
  const [winCondition, setWinCondition] = useState(game?.win_condition ?? "")
  const [notes, setNotes] = useState(game?.notes ?? "")

  function updateSeat(index: number, patch: Partial<DraftSeat>) {
    setSeats((current) =>
      current.map((seat, seatIndex) => (seatIndex === index ? { ...seat, ...patch } : seat)),
    )
  }

  return {
    playedAt,
    setPlayedAt,
    seats,
    setSeats,
    winnerSeatId,
    setWinnerSeatId,
    turns,
    setTurns,
    duration,
    setDuration,
    winCondition,
    setWinCondition,
    notes,
    setNotes,
    updateSeat,
  }
}
