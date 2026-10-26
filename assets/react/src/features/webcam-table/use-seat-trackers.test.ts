import { act, renderHook } from "@testing-library/react"
import { afterEach, expect, it } from "vite-plus/test"
import type { RoomLink } from "./room-link"
import { newCounter } from "./trackers"
import { useSeatTrackers } from "./use-seat-trackers"

afterEach(() => localStorage.clear())

it("the last-counter shortcut follows the counter last created or changed, else the newest", () => {
  const link: RoomLink = { peerId: "peer", channel: null, spectator: false, participants: [] }
  const { result } = renderHook(() => useSeatTrackers(link, "room", 7))
  const values = () =>
    result.current.trackers.counters.map(({ label, value }) => `${label}=${value}`)

  act(() => result.current.adjustLastCounter(1))
  expect(values()).toEqual([])

  const [lands, storm] = [newCounter(1), newCounter(2)]
  act(() => result.current.setTrackers((current) => ({ ...current, counters: [lands] })))
  act(() => result.current.setTrackers((current) => ({ ...current, counters: [lands, storm] })))
  act(() => result.current.adjustLastCounter(1))
  expect(values()).toEqual(["Counter 1=0", "Counter 2=1"])

  // Changing the older counter, by value or by name, makes it the target.
  act(() => result.current.adjustCounter(lands.id, 2))
  act(() => result.current.adjustLastCounter(1))
  expect(values()).toEqual(["Counter 1=3", "Counter 2=1"])
  act(() =>
    result.current.setTrackers((current) => ({
      ...current,
      counters: current.counters.map((counter) =>
        counter.id === storm.id ? { ...counter, label: "Storm" } : counter,
      ),
    })),
  )
  act(() => result.current.adjustLastCounter(-1))
  expect(values()).toEqual(["Counter 1=3", "Storm=0"])

  // Removing the target falls back to the newest remaining counter.
  const third = newCounter(3)
  act(() =>
    result.current.setTrackers((current) => ({
      ...current,
      counters: [current.counters[0]!, third],
    })),
  )
  act(() => result.current.adjustCounter(lands.id, 1))
  act(() =>
    result.current.setTrackers((current) => ({ ...current, counters: [current.counters[1]!] })),
  )
  act(() => result.current.adjustLastCounter(1))
  expect(values()).toEqual(["Counter 3=1"])
})
