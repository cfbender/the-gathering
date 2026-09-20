import { describe, expect, it } from "vite-plus/test"
import { colorWheelSlices, donutSlicePath } from "./color-wheel"

describe("colorWheelSlices", () => {
  it("uses color appearances as the denominator and derives cumulative angles", () => {
    const slices = colorWheelSlices([
      { id: "W", games: 1 },
      { id: "U", games: 2 },
      { id: "B", games: 3 },
      { id: "R", games: 4 },
      { id: "G", games: 10 },
    ])

    expect(slices.map(({ percentage }) => percentage)).toEqual([5, 10, 15, 20, 50])
    expect(slices.map(({ startAngle, endAngle }) => [startAngle, endAngle])).toEqual([
      [-90, -72],
      [-72, -36],
      [-36, 18],
      [18, 90],
      [90, 270],
    ])
  })

  it("produces zero-sized slices when no colors have games", () => {
    expect(colorWheelSlices([{ id: "W", games: 0 }])[0]).toMatchObject({
      percentage: 0,
      startAngle: -90,
      endAngle: -90,
    })
  })
})

describe("donutSlicePath", () => {
  it("omits a zero-sized slice and closes a full-circle slice", () => {
    expect(donutSlicePath(-90, -90)).toBe("")
    expect(donutSlicePath(-90, 270)).toMatch(/A 88 88.*A 48 48.*Z$/)
  })
})
