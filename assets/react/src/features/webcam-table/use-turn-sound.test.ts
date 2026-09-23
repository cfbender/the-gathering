import { fireEvent, renderHook } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { useTurnSound } from "./use-turn-sound"

afterEach(() => vi.unstubAllGlobals())

it("chimes only on a new local turn after interaction, and releases the audio context", () => {
  const start = vi.fn()
  const close = vi.fn().mockResolvedValue(undefined)
  const resume = vi.fn().mockResolvedValue(undefined)
  const frequency = { setValueAtTime: vi.fn() }
  const envelope = { setValueAtTime: vi.fn(), exponentialRampToValueAtTime: vi.fn() }
  class FakeAudioContext {
    state = "running"
    currentTime = 10
    destination = {}
    close = close
    resume = resume
    createOscillator() {
      return { frequency, connect: (gain: unknown) => gain, start, stop: vi.fn() }
    }
    createGain() {
      return { gain: envelope, connect: vi.fn() }
    }
  }
  vi.stubGlobal("AudioContext", FakeAudioContext)
  const hook = renderHook(({ enabled, active }) => useTurnSound(enabled, active, 7), {
    initialProps: { enabled: true, active: 7 },
  })
  expect(start).not.toHaveBeenCalled()
  hook.rerender({ enabled: true, active: 8 })
  hook.rerender({ enabled: true, active: 7 })
  expect(start).not.toHaveBeenCalled()
  fireEvent.pointerDown(window)
  hook.rerender({ enabled: true, active: 8 })
  expect(start).not.toHaveBeenCalled()
  hook.rerender({ enabled: true, active: 7 })
  expect(start).toHaveBeenCalledTimes(1)
  expect(frequency.setValueAtTime).toHaveBeenCalledWith(660, 10)
  expect(frequency.setValueAtTime).toHaveBeenCalledWith(880, 10.12)
  hook.rerender({ enabled: true, active: 7 })
  expect(start).toHaveBeenCalledTimes(1)
  hook.rerender({ enabled: false, active: 8 })
  expect(close).toHaveBeenCalledTimes(1)
  hook.rerender({ enabled: false, active: 7 })
  expect(start).toHaveBeenCalledTimes(1)
  hook.unmount()
})
