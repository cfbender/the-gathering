import { describe, expect, it } from "vite-plus/test"
import { CLEAR_MARGIN, isClear } from "./card-suggestions"
import type { Candidate } from "./recognition/pipeline"

function art(id: string, score: number): Candidate {
  return {
    id,
    name: id,
    set: "fic",
    collector_number: "1",
    layout: "normal",
    frame: "modern",
    index: 0,
    score,
  }
}

describe("isClear", () => {
  it("is clear only when the leader beats the runner-up by the margin", () => {
    // exactly representable binary fractions, so the boundary is not a rounding accident
    expect(CLEAR_MARGIN).toBeLessThan(0.125)
    expect(isClear([art("a", 0.75), art("b", 0.625)])).toBe(true)
    expect(isClear([art("a", 0.75), art("b", 0.75 - CLEAR_MARGIN / 2)])).toBe(false)
  })

  it("never auto-confirms a lone or absent candidate", () => {
    expect(isClear([])).toBe(false)
    expect(isClear([art("a", 0.95)])).toBe(false)
  })
})
