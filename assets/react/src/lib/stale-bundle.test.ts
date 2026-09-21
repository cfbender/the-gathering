import { describe, expect, it, vi } from "vite-plus/test"
import { RELOAD_KEY, RELOAD_WINDOW_MS, handleStaleBundle } from "@/lib/stale-bundle"

function memoryStorage(initial: Record<string, string> = {}) {
  const map = new Map(Object.entries(initial))
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => void map.set(key, value),
    removeItem: (key: string) => void map.delete(key),
    map,
  }
}

describe("handleStaleBundle", () => {
  it("reloads once and records when it did", () => {
    const storage = memoryStorage()
    const reload = vi.fn()

    expect(handleStaleBundle({ now: () => 1_000, reload, storage })).toBe(true)
    expect(reload).toHaveBeenCalledTimes(1)
    expect(storage.map.get(RELOAD_KEY)).toBe("1000")
  })

  it("does not reload again within the window", () => {
    const storage = memoryStorage({ [RELOAD_KEY]: "1000" })
    const reload = vi.fn()

    const at = 1_000 + RELOAD_WINDOW_MS - 1
    expect(handleStaleBundle({ now: () => at, reload, storage })).toBe(false)
    expect(reload).not.toHaveBeenCalled()
    expect(storage.map.get(RELOAD_KEY)).toBe("1000")
  })

  it("reloads again once the window has passed", () => {
    const storage = memoryStorage({ [RELOAD_KEY]: "1000" })
    const reload = vi.fn()

    const at = 1_000 + RELOAD_WINDOW_MS
    expect(handleStaleBundle({ now: () => at, reload, storage })).toBe(true)
    expect(reload).toHaveBeenCalledTimes(1)
    expect(storage.map.get(RELOAD_KEY)).toBe(String(at))
  })

  it("treats a corrupt marker as absent", () => {
    const storage = memoryStorage({ [RELOAD_KEY]: "not-a-number" })
    const reload = vi.fn()

    expect(handleStaleBundle({ now: () => 5_000, reload, storage })).toBe(true)
    expect(reload).toHaveBeenCalledTimes(1)
  })

  it("does not reload when storage is unavailable", () => {
    const reload = vi.fn()
    const storage = {
      getItem: () => {
        throw new Error("blocked")
      },
      setItem: () => {
        throw new Error("blocked")
      },
      removeItem: () => {},
    }

    expect(handleStaleBundle({ now: () => 5_000, reload, storage })).toBe(false)
    expect(reload).not.toHaveBeenCalled()
  })
})
