import { expect, it } from "vite-plus/test"
import { CLEAR_MARGIN, isClear } from "./card-suggestions"
import {
  applyDeckHint,
  DECK_PRIOR,
  deckFirst,
  deckHint,
  deckNames,
  inDeck,
  listPrintingIds,
} from "./deck-hint"
import type { Candidate, GalleryArt } from "./recognition/pipeline"

function candidate(name: string, score: number): Candidate {
  return { id: name, name, set: "tst", frame: "modern", index: 0, score }
}

const card = (name: string, printing_id: string | null = null) => ({
  name,
  quantity: 1,
  zone: "mainboard" as const,
  printing_id,
  card_id: null,
  type_line: null,
  mana_cost: null,
  cmc: null,
  game_changer: false,
  image_uris: {},
})

it("matches whole names, faces, case and accents", () => {
  const names = deckNames({ cards: [card("Brazen Borrower // Petty Theft"), card("Jötun Grunt")] })
  expect(inDeck("Brazen Borrower", names)).toBe(true)
  expect(inDeck("Petty Theft", names)).toBe(true)
  expect(inDeck("brazen borrower // petty theft", names)).toBe(true)
  expect(inDeck("Jotun Grunt", names)).toBe(true)
  expect(inDeck("Sol Ring", names)).toBe(false)
  expect(inDeck("Sol Ring", undefined)).toBe(false)
})

it("lifts deck cards by the prior and re-ranks", () => {
  const names = deckNames({ cards: [card("Sol Ring")] })
  const hinted = applyDeckHint(
    [candidate("Mind Stone", 0.72), candidate("Sol Ring", 0.7), candidate("Signet", 0.5)],
    names,
  )
  expect(hinted.map((art) => art.name)).toEqual(["Sol Ring", "Mind Stone", "Signet"])
  expect(hinted[0]?.score).toBeCloseTo(0.7 + DECK_PRIOR)
  expect(hinted[0]?.inDeck).toBe(true)
  expect(hinted[1]?.inDeck).toBeUndefined()
})

it("never overturns a lead larger than the prior", () => {
  const names = deckNames({ cards: [card("Sol Ring")] })
  const raw = [candidate("Mind Stone", 0.7 + DECK_PRIOR + 0.001), candidate("Sol Ring", 0.7)]
  expect(applyDeckHint(raw, names)[0]?.name).toBe("Mind Stone")
})

it("turns a deck card's near-tie into a clear answer only within the prior", () => {
  const names = deckNames({ cards: [card("Sol Ring")] })
  const nearTie = [candidate("Sol Ring", 0.8), candidate("Mind Stone", 0.8 - CLEAR_MARGIN + 0.02)]
  expect(isClear(nearTie)).toBe(false)
  expect(isClear(applyDeckHint(nearTie, names))).toBe(true)
  // Both in the deck: the prior cancels out and the margin is unchanged.
  const both = deckNames({ cards: [card("Sol Ring"), card("Mind Stone")] })
  expect(isClear(applyDeckHint(nearTie, both))).toBe(false)
})

it("returns candidates untouched without a list", () => {
  const raw = [candidate("A", 0.5), candidate("B", 0.4)]
  expect(applyDeckHint(raw, undefined)).toBe(raw)
  expect(applyDeckHint(raw, new Set())).toBe(raw)
})

it("puts deck cards first in search results, keeping order otherwise", () => {
  const names = deckNames({ cards: [card("Forest")] })
  const arts = [{ name: "Forest Bear" }, { name: "Forest" }, { name: "Forested Hill" }]
  expect(deckFirst(arts, names).map((art) => art.name)).toEqual([
    "Forest",
    "Forest Bear",
    "Forested Hill",
  ])
})

it("maps gallery arts to this list's own printings only", () => {
  const list = { cards: [card("Sol Ring", "p-mine"), card("Forest")] }
  expect(listPrintingIds(list)).toEqual(["p-mine"])
  const located: GalleryArt[] = [
    {
      id: "art-1",
      name: "Sol Ring",
      set: "c21",
      frame: "modern",
      printings: [
        { id: "p-theirs", name: "Sol Ring", set: "ltc" },
        { id: "p-mine", name: "Sol Ring", set: "cmm" },
      ],
    },
    {
      id: "art-2",
      name: "Sol Ring",
      set: "lea",
      frame: "1993",
      printings: [{ id: "p-other-seat", name: "Sol Ring", set: "lea" }],
    },
  ]
  const hint = deckHint(list, located)
  expect([...hint.printings.keys()]).toEqual(["art-1"])
  expect(hint.printings.get("art-1")).toEqual({
    id: "p-mine",
    name: "Sol Ring",
    set: "cmm",
    frame: "modern",
  })
})
