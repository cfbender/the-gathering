import { describe, expect, it } from "vite-plus/test"
import { linePoints } from "./stats"

describe("linePoints", () => {
  it("maps asymmetric percentage values to chart coordinates", () => {
    expect(linePoints([0, 25, 100], 200, 80)).toBe("0,80 100,60 200,0")
  })

  it("handles empty and single-value series", () => {
    expect(linePoints([])).toBe("")
    expect(linePoints([50], 200, 80)).toBe("0,40")
  })
})
