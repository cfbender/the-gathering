import { describe, expect, it } from "vite-plus/test"
import { cameraGridLayout } from "./camera-grid"

const cellsOf = (count: number) =>
  cameraGridLayout(count).cells.map(({ row, column }) => `${row},${column}`)

describe("camera grid layout", () => {
  it("seats a four-player pod clockwise from the top-left", () => {
    expect(cameraGridLayout(4)).toMatchObject({ columns: 2, rows: 2 })
    expect(cellsOf(4)).toEqual(["1,1", "1,2", "2,2", "2,1"])
  })

  it("keeps small tables on one row and leaves the last corner empty for three", () => {
    expect(cellsOf(1)).toEqual(["1,1"])
    expect(cellsOf(2)).toEqual(["1,1", "1,2"])
    expect(cellsOf(3)).toEqual(["1,1", "1,2", "2,2"])
  })

  it("wraps the bottom row back right to left", () => {
    expect(cameraGridLayout(6)).toMatchObject({ columns: 3, rows: 2 })
    expect(cellsOf(6)).toEqual(["1,1", "1,2", "1,3", "2,3", "2,2", "2,1"])
    expect(cellsOf(5)).toEqual(["1,1", "1,2", "1,3", "2,3", "2,2"])
  })

  it("walks the full ring and never seats anyone in the middle", () => {
    expect(cellsOf(8)).toEqual(["1,1", "1,2", "1,3", "2,3", "3,3", "3,2", "3,1", "2,1"])
    expect(cameraGridLayout(9)).toMatchObject({ columns: 4, rows: 3 })
    expect(cameraGridLayout(10)).toMatchObject({ columns: 4, rows: 3 })
    expect(cellsOf(10)).toEqual([
      "1,1",
      "1,2",
      "1,3",
      "1,4",
      "2,4",
      "3,4",
      "3,3",
      "3,2",
      "3,1",
      "2,1",
    ])
  })
})
