import type { TableParticipant } from "./use-webcam-room"

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
): TableParticipant | undefined {
  const start = seats.findIndex((seat) => seat.player_id === activeId) + 1
  return [...seats.slice(start), ...seats.slice(0, start)].find(
    (seat) => !seat.eliminated && !seat.departed,
  )
}

export function turnDisplay(turns: TurnState, playerId: number, gameElapsed: number) {
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

/** Shared metadata for the parent branch's hotkey-help registry. */
export const PASS_TURN_BINDING = { key: "Space", label: "Pass turn" } as const

export function canPassWithSpace(
  event: KeyboardEvent,
  pickerOpen: boolean,
  dialogOpen: boolean,
): boolean {
  if (
    event.code !== "Space" ||
    event.repeat ||
    event.defaultPrevented ||
    event.isComposing ||
    event.altKey ||
    event.ctrlKey ||
    event.metaKey ||
    event.shiftKey ||
    pickerOpen ||
    dialogOpen
  )
    return false
  const target = event.target
  return !(
    target instanceof Element &&
    target.closest(
      "input, textarea, select, button, a, [contenteditable]:not([contenteditable='false']), [role='textbox'], [role='combobox'], [role='menu']",
    )
  )
}
