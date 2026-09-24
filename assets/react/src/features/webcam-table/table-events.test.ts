import { describe, expect, it } from "vite-plus/test"
import {
  activeTurnOrder,
  orderBySeats,
  receiveTableEvent,
  retainEliminatedSeats,
  shuffleSeats,
  toTableEvent,
  type TableEvent,
} from "./table-events"
import type { TableParticipant } from "./use-webcam-room"
import { EMPTY_COUNTERS } from "./seat-counters"

function seat(overrides: Partial<TableParticipant> & { peer_id: string }): TableParticipant {
  return {
    ...EMPTY_COUNTERS,
    player_id: 1,
    player_name: "Alice",
    life: 40,
    camera_off: false,
    eliminated: false,
    joined_at: 1_000,
    ...overrides,
  }
}

describe("eliminated seats", () => {
  const alice = seat({ peer_id: "a", player_id: 1, eliminated: true })
  const bob = seat({ peer_id: "b", player_id: 2 })
  const cara = seat({ peer_id: "c", player_id: 3, eliminated: true })
  const dan = seat({ peer_id: "d", player_id: 4 })

  it("skips eliminated seats at the start and middle without changing recorded order", () => {
    const recorded = [alice, bob, cara, dan]
    expect(activeTurnOrder(recorded).map((p) => p.peer_id)).toEqual(["b", "d"])
    expect(recorded.map((p) => p.peer_id)).toEqual(["a", "b", "c", "d"])
    expect(activeTurnOrder([alice, cara])).toEqual([])
    expect(activeTurnOrder([alice, { ...cara, eliminated: false }])).toEqual([
      { ...cara, eliminated: false },
    ])
  })

  it("keeps departed eliminated seats and replaces them by player identity on rejoin", () => {
    const retained = retainEliminatedSeats([bob], [alice])
    expect(orderBySeats(retained, ["a", "b"])).toEqual([{ ...alice, departed: true }, bob])
    const rejoined = { ...alice, peer_id: "new-a" }
    expect(retainEliminatedSeats([bob, rejoined], [alice])).toEqual([bob, rejoined])
  })
})

describe("receiveTableEvent", () => {
  const entry = (id: number, text: string, count?: number): TableEvent =>
    toTableEvent({ id, at: id * 1000, text, count })

  it("prepends new server entries and replaces a merged head in place", () => {
    const joined = entry(1, "Alice joined the table")
    let events = receiveTableEvent([joined], entry(2, "Alice: 40 → 39 life"))
    events = receiveTableEvent(events, entry(2, "Alice: 40 → 37 life", 3))
    expect(events.map((event) => [event.text, event.count])).toEqual([
      ["Alice: 40 → 37 life", 3],
      ["Alice joined the table", undefined],
    ])
    expect(events[0]!.at).toEqual(new Date(2000))
  })

  it("keeps a snapshot entry once when its broadcast arrives too, and caps history", () => {
    const events = [entry(2, "b"), entry(1, "a")]
    expect(receiveTableEvent(events, entry(1, "a"))).toEqual(events)
    const full = Array.from({ length: 200 }, (_, index) => entry(200 - index, `line ${index}`))
    const next = receiveTableEvent(full, entry(201, "newest"))
    expect(next).toHaveLength(200)
    expect(next[0]!.text).toBe("newest")
  })
})

describe("orderBySeats", () => {
  const alice = seat({ peer_id: "a", joined_at: 3_000 })
  const bob = seat({ peer_id: "b", player_name: "Bob", joined_at: 1_000 })
  const cara = seat({ peer_id: "c", player_name: "Cara", joined_at: 2_000 })

  it("applies the shared order, skips departed peers, and appends newcomers", () => {
    expect(orderBySeats([alice, bob, cara], ["c", "ghost", "a"]).map((p) => p.peer_id)).toEqual([
      "c",
      "a",
      "b",
    ])
  })

  it("seats unordered players by join time regardless of presence order", () => {
    expect(orderBySeats([alice, cara, bob], []).map((p) => p.peer_id)).toEqual(["b", "c", "a"])
  })

  it("appends newcomers after the shared order in join order", () => {
    expect(orderBySeats([alice, bob, cara], ["a"]).map((p) => p.peer_id)).toEqual(["a", "b", "c"])
  })
})

describe("shuffleSeats", () => {
  it("returns a permutation that differs from the input", () => {
    const values = [0.99, 0.99, 0.99, 0.01, 0.5, 0.5]
    let index = 0
    const random = () => values[index++ % values.length] ?? 0.5
    const shuffled = shuffleSeats(["a", "b", "c"], random)

    expect([...shuffled].sort()).toEqual(["a", "b", "c"])
    expect(shuffled).not.toEqual(["a", "b", "c"])
  })

  it("retries when the first draw reproduces the current order", () => {
    const draws = [0, 0, 0.99, 0]
    let index = 0
    const shuffled = shuffleSeats(["a", "b"], () => draws[index++] ?? 0)

    expect(shuffled).toEqual(["b", "a"])
  })

  it("leaves a single seat alone", () => {
    expect(shuffleSeats(["solo"])).toEqual(["solo"])
  })
})
