import { describe, expect, it } from "vite-plus/test"
import { tablePlayerNames, type ActiveTable } from "./rooms"

function table(...names: string[]): ActiveTable {
  return {
    id: "room",
    started_at: 0,
    full: names.length >= 4,
    players: names.map((name, id) => ({ id, name })),
  }
}

describe("tablePlayerNames", () => {
  it("labels a table no one is seated at", () => {
    expect(tablePlayerNames(table())).toBe("Empty table")
  })

  it("names one player plainly", () => {
    expect(tablePlayerNames(table("Theo"))).toBe("Theo")
  })

  it("joins two with an ampersand", () => {
    expect(tablePlayerNames(table("Theo", "Mara"))).toBe("Theo & Mara")
  })

  it("uses commas before the last name", () => {
    expect(tablePlayerNames(table("Theo", "Mara", "Cody"))).toBe("Theo, Mara & Cody")
  })
})
