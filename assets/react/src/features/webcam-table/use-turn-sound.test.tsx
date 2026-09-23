import { act, cleanup, fireEvent, renderHook } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { useTurnSound } from "./use-turn-sound"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

it("waits for a successful gesture, silently retries blocked audio, and only chimes on a new local turn", async () => {
  const start = vi.fn()
  const context = {
    state: "suspended",
    currentTime: 0,
    destination: {},
    resume: vi
      .fn()
      .mockRejectedValueOnce(new Error("blocked"))
      .mockImplementation(async () => {
        context.state = "running"
      }),
    close: vi.fn().mockRejectedValue(new Error("already closed")),
    createOscillator: () => ({
      frequency: { setValueAtTime: vi.fn() },
      connect: () => ({ connect: vi.fn() }),
      start,
      stop: vi.fn(),
      disconnect: vi.fn(),
    }),
    createGain: () => ({
      gain: { setValueAtTime: vi.fn(), exponentialRampToValueAtTime: vi.fn() },
      disconnect: vi.fn(),
    }),
  }
  const Constructor = vi.fn(function () {
    return context
  })
  vi.stubGlobal("AudioContext", Constructor)
  const view = renderHook(({ active }) => useTurnSound(true, active, 1), {
    initialProps: { active: 2 },
  })
  view.rerender({ active: 1 })
  expect(Constructor).not.toHaveBeenCalled()
  await act(async () => {
    fireEvent.pointerDown(window)
  })
  view.rerender({ active: 2 })
  view.rerender({ active: 1 })
  expect(start).not.toHaveBeenCalled()
  await act(async () => {
    fireEvent.keyDown(window, { key: "a" })
  })
  expect(start).not.toHaveBeenCalled()
  view.rerender({ active: 2 })
  view.rerender({ active: 1 })
  expect(start).toHaveBeenCalledOnce()
  view.rerender({ active: 1 })
  expect(start).toHaveBeenCalledOnce()
  await act(async () => view.unmount())
  expect(context.close).toHaveBeenCalledOnce()
})
