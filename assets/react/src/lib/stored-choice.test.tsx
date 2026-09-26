import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vite-plus/test"
import { useStoredChoice } from "@/lib/stored-choice"

type Sort = "games" | "win_rate"
const isSort = (value: unknown): value is Sort => value === "games" || value === "win_rate"
const KEY = "the-gathering:test-sort"

describe("useStoredChoice", () => {
  afterEach(() => {
    cleanup()
    localStorage.clear()
  })

  it("starts at the default", () => {
    const { result } = renderHook(() => useStoredChoice<Sort>(KEY, "games", isSort))
    expect(result.current[0]).toBe("games")
  })

  it("remembers the choice for the next mount", () => {
    const first = renderHook(() => useStoredChoice<Sort>(KEY, "games", isSort))
    act(() => first.result.current[1]("win_rate"))
    expect(first.result.current[0]).toBe("win_rate")
    first.unmount()

    const second = renderHook(() => useStoredChoice<Sort>(KEY, "games", isSort))
    expect(second.result.current[0]).toBe("win_rate")
  })

  it("keeps separate keys independent", () => {
    const first = renderHook(() => useStoredChoice<Sort>(KEY, "games", isSort))
    act(() => first.result.current[1]("win_rate"))

    const other = renderHook(() => useStoredChoice<Sort>(`${KEY}:other`, "games", isSort))
    expect(other.result.current[0]).toBe("games")
  })

  it("clears storage when switching back to the default", () => {
    const { result } = renderHook(() => useStoredChoice<Sort>(KEY, "games", isSort))
    act(() => result.current[1]("win_rate"))
    expect(localStorage.getItem(KEY)).toBe("win_rate")
    act(() => result.current[1]("games"))
    expect(localStorage.getItem(KEY)).toBeNull()
  })

  it("ignores invalid stored values", () => {
    localStorage.setItem(KEY, "bogus")
    const { result } = renderHook(() => useStoredChoice<Sort>(KEY, "games", isSort))
    expect(result.current[0]).toBe("games")
  })
})
