import { describe, expect, it } from "vite-plus/test"
import { elapsedMilliseconds } from "./game-timer"
import { EMPTY_TURNS, formatTurnTime, nextActiveSeat, turnDisplay, type TurnState } from "./turns"
import { EMPTY_COUNTERS } from "./seat-counters"
import type { TableParticipant } from "./use-webcam-room"

const seats: TableParticipant[] = [1, 2, 3, 4].map((id) => ({
  ...EMPTY_COUNTERS,
  peer_id: String(id),
  player_id: id,
  player_name: `Player ${id}`,
  life: 40,
  camera_off: false,
  eliminated: id === 2,
  departed: id === 3,
  joined_at: id,
}))

describe("next turn", () => {
  it("skips eliminated and departed seats, wraps, and handles no eligible seats", () => {
    expect(nextActiveSeat(seats, null)?.player_id).toBe(1)
    expect(nextActiveSeat(seats, 1)?.player_id).toBe(4)
    expect(nextActiveSeat(seats, 4)?.player_id).toBe(1)
    expect(nextActiveSeat([seats[1]!, seats[2]!], 2)).toBeUndefined()
    expect(nextActiveSeat([seats[0]!], 1)?.player_id).toBe(1)
    expect(nextActiveSeat([], null)).toBeUndefined()
  })

  it("displays authoritative turn counts and unequal banked/active times across pass and pause", () => {
    expect(turnDisplay(EMPTY_TURNS, 1, 0)).toEqual({ count: 0, milliseconds: 0 })
    // Server snapshot after player 1 spent 12s, then player 4 spent 19s.
    const turns: TurnState = {
      active_player_id: 1,
      counts: { 1: 2, 4: 1 },
      elapsed_ms: { 1: 12000, 4: 19000 },
      started_elapsed_ms: 31000,
      revision: 3,
    }
    const timer = { started_at: 1000, paused_ms: 9000, paused_at: 46000, server_now: 100000 }
    const elapsed = elapsedMilliseconds(timer)
    expect(turnDisplay(turns, 1, elapsed)).toEqual({ count: 2, milliseconds: 17000 })
    expect(turnDisplay(turns, 4, elapsed)).toEqual({ count: 1, milliseconds: 19000 })
    expect(turnDisplay(turns, 1, elapsedMilliseconds(timer, 150000)).milliseconds).toBe(17000)
    const resumed = { ...timer, paused_at: null, paused_ms: 63000, server_now: 107000 }
    expect(turnDisplay(turns, 1, elapsedMilliseconds(resumed))).toEqual({
      count: 2,
      milliseconds: 24000,
    })
    expect(turnDisplay(turns, 2, elapsedMilliseconds(resumed))).toEqual({
      count: 0,
      milliseconds: 0,
    })
    expect(formatTurnTime(125999)).toBe("2:05")
  })
})
