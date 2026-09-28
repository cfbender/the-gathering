/** Milliseconds on the server clock; no browser wall-clock timestamps participate. */
export interface GameTimerState {
  started_at: number | null
  paused_at: number | null
  paused_ms: number
  server_now: number
}

export interface TimerSample {
  state: GameTimerState
  receivedAt: number
}

/** The game has started but its clock has never run: the mulligan window before the first turn.
 * Mirrors `Timer.awaiting_start?/1` on the server. */
export function awaitingStart(timer: GameTimerState | undefined): boolean {
  return timer?.started_at != null && timer.paused_at === timer.started_at
}

export function elapsedMilliseconds(timer: GameTimerState, serverNow = timer.server_now): number {
  if (timer.started_at === null) return 0
  return Math.max(0, (timer.paused_at ?? serverNow) - timer.started_at - timer.paused_ms)
}

/** Advance from a server sample with a monotonic clock, periodically resynced by the channel. */
export function sampledElapsed(sample: TimerSample, monotonicNow: number): number {
  return elapsedMilliseconds(
    sample.state,
    sample.state.server_now + monotonicNow - sample.receivedAt,
  )
}

export function durationMinutes(timer: GameTimerState): string {
  return timer.started_at === null
    ? ""
    : String(Math.max(1, Math.round(elapsedMilliseconds(timer) / 60_000)))
}

export function formatElapsed(milliseconds: number): string {
  const seconds = Math.floor(milliseconds / 1000)
  return [Math.floor(seconds / 3600), Math.floor((seconds % 3600) / 60), seconds % 60]
    .map((value) => String(value).padStart(2, "0"))
    .join(":")
}
