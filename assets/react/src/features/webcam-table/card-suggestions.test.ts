import { describe, expect, it } from "vite-plus/test"
import { CLEAR_MARGIN, isClear } from "./card-suggestions"
import type { Identification } from "./recognition/messages"

type Candidate = Identification["candidates"][number]

function candidate(score: number, index: number): Candidate {
  return {
    id: `art-${index}`,
    name: `Card ${index}`,
    set: "fic",
    collector_number: String(index),
    frame: "2015",
    score,
    index,
  }
}

describe("isClear", () => {
  it("is clear when top-1 leads the runner-up by more than the margin", () => {
    expect(isClear([candidate(0.75, 0), candidate(0.625, 1)])).toBe(true)
    expect(isClear([candidate(0.75, 0), candidate(0.75 - CLEAR_MARGIN * 1.5, 1)])).toBe(true)
  })

  it("is a near-tie just inside the margin", () => {
    expect(isClear([candidate(0.75, 0), candidate(0.75 - CLEAR_MARGIN / 2, 1)])).toBe(false)
    expect(isClear([candidate(0.75, 0), candidate(0.75, 1)])).toBe(false)
  })

  it("never treats a lone or empty result as clear", () => {
    expect(isClear([])).toBe(false)
    expect(isClear([candidate(0.9, 0)])).toBe(false)
  })
})
