import type { TableParticipant } from "./use-webcam-room"
import type { GameFormat } from "@/features/games/game-format"
import { teams, turnId } from "./game-modes"

export interface TurnState {
  active_player_id: number | null
  counts: Record<number, number>
  elapsed_ms: Record<number, number>
  started_elapsed_ms: number
  revision: number
}

export const EMPTY_TURNS: TurnState = {
  active_player_id: null,
  counts: {},
  elapsed_ms: {},
  started_elapsed_ms: 0,
  revision: 0,
}

/** After the current player, wrapping once; never suggest an out or departed seat. */
export function nextActiveSeat(
  seats: TableParticipant[],
  activeId: number | null,
  mode: GameFormat = "commander",
): TableParticipant | undefined {
  if (mode === "two_headed_giant") {
    activeId = turnId(seats, activeId, mode)
    seats = teams(seats).map((team) => ({
      ...team[0]!,
      eliminated: team.every((seat) => seat.eliminated || seat.departed),
      departed: false,
    }))
  }
  const start = seats.findIndex((seat) => seat.player_id === activeId) + 1
  return [...seats.slice(start), ...seats.slice(0, start)].find(
    (seat) => !seat.eliminated && !seat.departed,
  )
}

export function turnDisplay(
  turns: TurnState,
  playerId: number,
  gameElapsed: number,
  seats: TableParticipant[] = [],
  mode: GameFormat = "commander",
) {
  playerId = turnId(seats, playerId, mode) ?? playerId
  const running =
    turns.active_player_id === playerId ? Math.max(0, gameElapsed - turns.started_elapsed_ms) : 0
  return {
    count: turns.counts[playerId] ?? 0,
    milliseconds: (turns.elapsed_ms[playerId] ?? 0) + running,
  }
}

export function formatTurnTime(milliseconds: number): string {
  const seconds = Math.floor(milliseconds / 1000)
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`
}
