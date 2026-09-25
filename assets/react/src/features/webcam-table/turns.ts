import type { TableParticipant } from "./use-webcam-room"
import type { GameFormat } from "@/features/games/game-format"
import { teams, turnId } from "./game-modes"

export interface TurnState {
  active_player_id: number | null
  counts: Record<number, number>
  elapsed_ms: Record<number, number>
  started_elapsed_ms: number
  revision: number
  /** Recent passes, newest first; the server uses them to undo a pass. */
  history: { player_id: number; started_elapsed_ms: number; next_player_id: number | null }[]
}

export const EMPTY_TURNS: TurnState = {
  active_player_id: null,
  counts: {},
  elapsed_ms: {},
  started_elapsed_ms: 0,
  revision: 0,
  history: [],
}

/** The player an un-pass would hand the turn back to, if the last pass led to the current turn.
 * The server still refuses when that player has since been eliminated or left. */
export function unpassTarget(turns: TurnState): number | null {
  const [last] = turns.history
  return last && last.next_player_id === turns.active_player_id ? last.player_id : null
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
