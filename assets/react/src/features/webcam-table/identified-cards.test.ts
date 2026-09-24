import { describe, expect, it } from "vite-plus/test"
import {
  applyCardCommand,
  clearBoardCards,
  mergeIdentifiedCards,
  sameCard,
} from "./identified-cards"
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

  it("keeps two printed faces distinct but dedupes another printing of the same face", () => {
    const front = entry("front", "Jadzi, Oracle of Arcavios")
    const back = entry("back", "Journey to the Oracle")
    front.card.id = "b0a96416-9ee5-4202-a99f-e09db8794567"
    back.card.id = `${front.card.id}-1`
    expect(mergeIdentifiedCards([front], [back])).toEqual([back, front])
    expect(
      mergeIdentifiedCards(
        [front, back],
        [entry("repeat", " JOURNEY TO THE ORACLE ", "alice", 20)],
      ),
    ).toEqual([back, front])
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

  it("clears one board and leaves the rest of the table alone", () => {
    const mine = entry("mine")
    const theirs = entry("theirs", "Counterspell", "bob")
    expect(clearBoardCards([mine, theirs], "alice")).toEqual([theirs])
    expect(clearBoardCards([theirs], "alice")).toEqual([theirs])
  })

  it("overlays pending commands on the server's list", () => {
    const mine = entry("mine")
    const theirs = entry("theirs", "Counterspell", "bob")
    const added = entry("added", "Island", "alice", 30)
    expect(applyCardCommand([mine], { type: "card_identified", entry: added })).toEqual([
      mine,
      added,
    ])
    expect(applyCardCommand([mine, theirs], { type: "card_removed", id: "mine" })).toEqual([theirs])
    expect(applyCardCommand([mine, theirs], { type: "cards_cleared", ownerPeerId: "bob" })).toEqual(
      [mine],
    )
  })
})
