import { describe, expect, it } from "vite-plus/test"
import {
  activeTurnOrder,
  appendTableEvent,
  describeParticipantChange,
  describeParticipantLeft,
  orderBySeats,
  retainEliminatedSeats,
  shuffleSeats,
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

describe("describeParticipantChange", () => {
  it("announces a brand-new seat", () => {
    expect(describeParticipantChange(undefined, seat({ peer_id: "a" }))).toEqual([
      { text: "Alice joined the table" },
    ])
  })

  it("lists every changed fact and nothing else", () => {
    const before = seat({ peer_id: "a" })
    const after = seat({ peer_id: "a", life: 37, deck_id: 4, deck_name: "Birds", camera_off: true })

    expect(describeParticipantChange(before, after).map((event) => event.text)).toEqual([
      "Alice chose Birds",
      "Alice: 40 → 37 life",
      "Alice turned their camera off",
    ])
    expect(describeParticipantChange(after, after)).toEqual([])
    expect(describeParticipantLeft(after)).toBe("Alice left the table")
  })

  it("logs elimination and restoration without inventing a life change", () => {
    const before = seat({ peer_id: "a" })
    const out = { ...before, eliminated: true }
    expect(describeParticipantChange(before, out).map((event) => event.text)).toEqual([
      "Alice was eliminated",
    ])
    expect(describeParticipantChange(out, before).map((event) => event.text)).toEqual([
      "Alice was restored to the game",
    ])
    const changes = [
      describeParticipantChange(before, out)[0]!,
      describeParticipantChange(out, before)[0]!,
    ]
    expect(
      changes
        .map((change, id) => ({ ...change, id, at: new Date(id) }))
        .reduce(appendTableEvent, [] as TableEvent[]),
    ).toHaveLength(2)
  })
})

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

describe("appendTableEvent", () => {
  const life = (from: number, to: number, at: number, actor = "a"): TableEvent => ({
    id: at,
    at: new Date(at),
    text: `Alice: ${from} → ${to} life`,
    actor,
    kind: "life",
    life: { name: "Alice", from, to },
  })

  it("coalesces rapid changes from the original total through the final total", () => {
    const events = [life(40, 39, 1000), life(39, 38, 1500), life(38, 37, 2000)].reduce(
      appendTableEvent,
      [] as TableEvent[],
    )
    expect(events).toHaveLength(1)
    expect(events[0]).toMatchObject({ id: 1000, text: "Alice: 40 → 37 life", count: 3 })
  })

  it("merges at the window boundary but not beyond it or backwards in time", () => {
    expect(appendTableEvent([life(40, 39, 1000)], life(39, 35, 3000))).toHaveLength(1)
    expect(appendTableEvent([life(40, 39, 1000)], life(39, 35, 3001))).toHaveLength(2)
    expect(appendTableEvent([life(40, 39, 1000)], life(39, 35, 999))).toHaveLength(2)
  })

  it("does not cross an actor, kind, or intervening event", () => {
    const first = life(40, 39, 1000)
    const other = life(40, 38, 1100, "b")
    const changedKind = { ...other, actor: "a", kind: "camera", life: undefined }
    for (const between of [
      other,
      changedKind,
      { id: 1100, at: new Date(1100), text: "Seat order randomized" },
    ]) {
      const events = [first, between, life(39, 37, 1200)].reduce(
        appendTableEvent,
        [] as TableEvent[],
      )
      expect(events).toHaveLength(3)
    }
  })

  it("preserves every dice result and caps history without mutating it", () => {
    const prefix = "Alice rolled a d20: "
    const makeRoll = (result: number, at: number): TableEvent => ({
      id: at,
      at: new Date(at),
      actor: "a",
      kind: "dice:20",
      text: prefix + result,
      roll: { prefix, results: [result] },
    })
    const first = makeRoll(17, 1000)
    const result = appendTableEvent([first], makeRoll(3, 1200))
    expect(result[0]?.text).toBe("Alice rolled a d20: 17, 3")
    expect(first.roll?.results).toEqual([17])
    const history = Array.from({ length: 200 }, (_, id) => ({
      id,
      at: new Date(id),
      text: "Joined",
    }))
    expect(appendTableEvent(history, first)).toHaveLength(200)
    expect(history).toHaveLength(200)
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
