import { describe, expect, it } from "vite-plus/test"
import { commanderBackground } from "./commander-colors"

describe("commander backgrounds", () => {
  it("uses neutral for colorless and unknown identities, and solid mono-colors", () => {
    expect(commanderBackground("")).toBe("#18181b")
    expect(commanderBackground("C")).toBe("#18181b")
    expect(commanderBackground("?")).toBe("#18181b")
    expect(commanderBackground("W")).toBe("#2b2818")
    expect(commanderBackground("U")).toBe("#0b1a2e")
    expect(commanderBackground("B")).toBe("#1a1421")
    expect(commanderBackground("R")).toBe("#290c0f")
    expect(commanderBackground("G")).toBe("#0a1c10")
  })

  it("deduplicates and orders gradients WUBRG rather than using incoming order", () => {
    expect(commanderBackground("GRRG")).toBe("linear-gradient(90deg, #290c0f, #0a1c10)")
    expect(commanderBackground("GUWBR")).toBe(
      "linear-gradient(90deg, #2b2818, #0b1a2e, #1a1421, #290c0f, #0a1c10)",
    )
    expect(commanderBackground("UU")).toBe("#0b1a2e")
  })
})
