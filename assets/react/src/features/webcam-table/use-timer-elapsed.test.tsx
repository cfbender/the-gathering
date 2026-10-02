import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import type { TimerSample } from "./game-timer"
import { useTimerElapsed } from "./use-timer-elapsed"

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

function sample(state: Partial<TimerSample["state"]>): TimerSample {
  return {
    state: { started_at: null, paused_at: null, paused_ms: 0, server_now: 10_000, ...state },
    receivedAt: performance.now(),
  }
}

it("advances once a second while the clock runs", () => {
  const running = sample({ started_at: 4_000 })
  const view = renderHook(({ timer }) => useTimerElapsed(timer), {
    initialProps: { timer: running as TimerSample | null },
  })
  expect(view.result.current).toBe(6_000)
  act(() => {
    vi.advanceTimersByTime(999)
  })
  expect(view.result.current).toBe(6_000)
  act(() => {
    vi.advanceTimersByTime(1)
  })
  expect(view.result.current).toBe(7_000)
  act(() => {
    vi.advanceTimersByTime(2_000)
  })
  expect(view.result.current).toBe(9_000)
})

it("does not tick before the game starts or while it is paused", () => {
  const view = renderHook(({ timer }) => useTimerElapsed(timer), {
    initialProps: { timer: null as TimerSample | null },
  })
  expect(view.result.current).toBe(0)
  expect(vi.getTimerCount()).toBe(0)

  view.rerender({ timer: sample({ started_at: 4_000, paused_at: 9_000 }) })
  expect(view.result.current).toBe(5_000)
  expect(vi.getTimerCount()).toBe(0)
  act(() => {
    vi.advanceTimersByTime(5_000)
  })
  expect(view.result.current).toBe(5_000)

  // Resuming starts the tick again from the resumed sample, not from a stale clock.
  view.rerender({ timer: sample({ started_at: 4_000, paused_ms: 3_000, server_now: 20_000 }) })
  expect(view.result.current).toBe(13_000)
  expect(vi.getTimerCount()).toBe(1)
  act(() => {
    vi.advanceTimersByTime(1_000)
  })
  expect(view.result.current).toBe(14_000)
})
