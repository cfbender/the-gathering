import { describe, expect, it } from "vite-plus/test"
import {
  describeCardIdentified,
  describeParticipantChange,
  describeParticipantLeft,
  orderBySeats,
  shuffleSeats,
} from "./table-events"
import type { TableParticipant } from "./use-webcam-room"

function seat(overrides: Partial<TableParticipant> & { peer_id: string }): TableParticipant {
  return {
    player_id: 1,
    player_name: "Alice",
    life: 40,
    camera_off: false,
    joined_at: 1_000,
    ...overrides,
  }
}

describe("describeCardIdentified", () => {
  const card = { id: "x", name: "Command Tower", set: "fic", collector_number: "301" }

  it("names the card, printing and whose board it was on", () => {
    expect(describeCardIdentified("Cody", seat({ peer_id: "m", player_name: "Mara" }), card)).toBe(
      "Cody identified Command Tower [FIC #301] on Mara's board",
    )
  })

  it("handles your own board and a missing collector number", () => {
    expect(
      describeCardIdentified("Alice", seat({ peer_id: "a" }), {
        ...card,
        collector_number: undefined,
      }),
    ).toBe("Alice identified Command Tower [FIC] on their board")
    expect(describeCardIdentified("Alice", undefined, card)).toBe(
      "Alice identified Command Tower [FIC #301]",
    )
  })
})

describe("describeParticipantChange", () => {
  it("announces a brand-new seat", () => {
    expect(describeParticipantChange(undefined, seat({ peer_id: "a" }))).toEqual([
      "Alice joined the table",
    ])
  })

  it("lists every changed fact and nothing else", () => {
    const before = seat({ peer_id: "a" })
    const after = seat({ peer_id: "a", life: 37, deck_id: 4, deck_name: "Birds", camera_off: true })

    expect(describeParticipantChange(before, after)).toEqual([
      "Alice chose Birds",
      "Alice 40 → 37 life",
      "Alice turned their camera off",
    ])
    expect(describeParticipantChange(after, after)).toEqual([])
    expect(describeParticipantLeft(after)).toBe("Alice left the table")
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
