import type { TableParticipant } from "./use-webcam-room"
import { activeTurnOrder } from "./table-events"
import type { GameFormat } from "@/features/games/game-format"
import { teams } from "./game-modes"

export function suggestedWinner(
  participants: TableParticipant[],
  mode: GameFormat = "commander",
): string {
  if (mode === "two_headed_giant") {
    const remaining = teams(participants).filter((team) => team.some((seat) => !seat.eliminated))
    return participants.length >= 4 && remaining.length === 1 ? remaining[0]![0]!.peer_id : ""
  }
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

export function buildGamePayload(
  participants: TableParticipant[],
  details: ResultDetails,
  mode: GameFormat = "commander",
) {
  const winners =
    mode === "two_headed_giant"
      ? (teams(participants)
          .find((team) => team[0]?.peer_id === details.winner)
          ?.map((seat) => seat.peer_id) ?? [])
      : [details.winner]
  return {
    game: {
      format: mode,
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
            : winners.includes(participant.peer_id)
              ? ("win" as const)
              : ("loss" as const),
      })),
    },
  }
}
