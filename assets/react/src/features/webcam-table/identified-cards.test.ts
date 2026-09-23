import { describe, expect, it } from "vite-plus/test"
import { mergeIdentifiedCards, sameCard } from "./identified-cards"
import type { BoardCard } from "./use-webcam-room"

function entry(id: string, name = "Lightning Bolt", ownerPeerId = "alice", at = 10): BoardCard {
  return {
    id,
    ownerPeerId,
    at,
    byPlayerName: "Bob",
    card: { id: `printing-${id}`, name, set: "lea" },
  }
}

describe("identified cards", () => {
  it("keeps the first printing of a card on each board across clicks and syncs", () => {
    const first = entry("first")
    const otherBoard = entry("other-board", "Lightning Bolt", "bob")
    const otherCard = entry("other-card", "Counterspell")
    expect(
      mergeIdentifiedCards(
        [first],
        [entry("repeat", " LIGHTNING BOLT ", "alice", 20), otherBoard, otherCard],
      ),
    ).toEqual([first, otherBoard, otherCard])
    expect(mergeIdentifiedCards([first], [first])).toEqual([first])
  })

  it("converges regardless of delivery order, including simultaneous discoveries", () => {
    const a = entry("a")
    const b = entry("b")
    const older = entry("z", "Lightning Bolt", "alice", 9)
    expect(mergeIdentifiedCards([b], [a])).toEqual([a])
    expect(mergeIdentifiedCards([a], [b])).toEqual([a])
    expect(mergeIdentifiedCards([a], [older])).toEqual([older])
  })

  it("reuses the replacement already on the board after removing a wrong entry", () => {
    const correct = entry("correct", "Counterspell")
    const wrong = entry("wrong")
    const remaining = [wrong, correct].filter((card) => card.id !== wrong.id)
    expect(mergeIdentifiedCards(remaining, [entry("choice", "Counterspell", "alice", 30)])).toEqual(
      [correct],
    )
    expect(sameCard(entry("a", "Fire // Ice").card, entry("b", "Fire").card)).toBe(false)
  })
})
