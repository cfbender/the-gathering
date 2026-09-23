import { describe, expect, it } from "vite-plus/test"
import {
  durationMinutes,
  elapsedMilliseconds,
  formatElapsed,
  sampledElapsed,
  type GameTimerState,
} from "./game-timer"
import { describeRoll } from "./table-rolls"

const timer: GameTimerState = {
  started_at: 1000,
  paused_at: null,
  paused_ms: 17000,
  server_now: 112000,
}

describe("game timer", () => {
  it("subtracts all previous pauses and freezes at the current pause", () => {
    expect(elapsedMilliseconds(timer)).toBe(94000)
    expect(elapsedMilliseconds({ ...timer, paused_at: 90000 }, 999999)).toBe(72000)
    expect(elapsedMilliseconds({ ...timer, started_at: null })).toBe(0)
  })

  it("interpolates on a monotonic clock without using the browser wall clock", () => {
    const sample = { state: timer, receivedAt: 500 }
    expect(sampledElapsed(sample, 2250)).toBe(95750)
    expect(sampledElapsed({ ...sample, state: { ...timer, paused_at: 90000 } }, 2250)).toBe(72000)
  })

  it("prefills editable whole minutes, with a minimum of one only for a started timer", () => {
    expect(durationMinutes(timer)).toBe("2")
    expect(durationMinutes({ ...timer, server_now: 107999 })).toBe("1")
    expect(durationMinutes({ ...timer, server_now: 108000 })).toBe("2")
    expect(durationMinutes({ ...timer, server_now: 1000, paused_ms: 0 })).toBe("1")
    expect(durationMinutes({ ...timer, started_at: null })).toBe("")
  })

  it("formats hours, minutes, and seconds without rounding up", () => {
    expect(formatElapsed(3723999)).toBe("01:02:03")
    expect(formatElapsed(0)).toBe("00:00:00")
  })
})

it("attributes dice and coin results", () => {
  const actor = { id: "r", actor: "a", player_name: "Cody", at: 1000 }
  expect(describeRoll({ ...actor, kind: "dice", sides: 20, result: 17 })).toBe(
    "Cody rolled a d20: 17",
  )
  expect(describeRoll({ ...actor, kind: "coin", result: "Tails" })).toBe(
    "Cody flipped a coin: Tails",
  )
})
