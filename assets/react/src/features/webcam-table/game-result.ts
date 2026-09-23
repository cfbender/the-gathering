import type { TableParticipant } from "./use-webcam-room"
import { activeTurnOrder } from "./table-events"

export function suggestedWinner(participants: TableParticipant[]): string {
  const remaining = activeTurnOrder(participants)
  return participants.length >= 2 && remaining.length === 1 ? remaining[0]!.peer_id : ""
}

interface ResultDetails {
  playedAt: Date
  winner: string
  duration: string
  turns: string
  winCondition: string
  notes: string
}

export function buildGamePayload(participants: TableParticipant[], details: ResultDetails) {
  return {
    game: {
      played_at: details.playedAt.toISOString(),
      duration_minutes: details.duration ? Number(details.duration) : null,
      turns: details.turns ? Number(details.turns) : null,
      win_condition: details.winCondition || null,
      notes: details.notes.trim() || null,
      seats: participants.map((participant, index) => ({
        player_id: participant.player_id,
        deck_id: participant.deck_id ?? null,
        seat: index + 1,
        result:
          details.winner === "draw"
            ? ("draw" as const)
            : details.winner === participant.peer_id
              ? ("win" as const)
              : ("loss" as const),
      })),
    },
  }
}
