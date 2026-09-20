import { describe, expect, it } from "vite-plus/test"
import { leaderboardRows, linePoints, sortByMetric, type NamedRecordRow } from "./stats"

describe("linePoints", () => {
  it("maps asymmetric percentage values to chart coordinates", () => {
    expect(linePoints([0, 25, 100], 200, 80)).toBe("0,80 100,60 200,0")
  })

  it("handles empty and single-value series", () => {
    expect(linePoints([])).toBe("")
    expect(linePoints([50], 200, 80)).toBe("0,40")
  })
})

const row = (id: string, games: number, wins: number): NamedRecordRow => ({
  id,
  name: id,
  games,
  wins,
  losses: games - wins,
  draws: 0,
  win_rate: games === 0 ? 0 : Math.round((wins * 1000) / games) / 10,
})

describe("leaderboardRows", () => {
  it("drops players below the game floor and ranks the rest by win rate", () => {
    const rows = [row("one-and-done", 1, 1), row("steady", 10, 5), row("hot", 4, 3)]
    expect(leaderboardRows(rows).map((r) => r.id)).toEqual(["hot", "steady"])
  })

  it("keeps players exactly at the floor and breaks ties by games played", () => {
    const rows = [row("two", 2, 1), row("eight", 8, 4)]
    expect(leaderboardRows(rows).map((r) => r.id)).toEqual(["eight", "two"])
  })
})

describe("sortByMetric", () => {
  const rows = [row("WRG", 6, 2), row("BG", 3, 3), row("UG", 6, 4)]

  it("sorts by popularity with win rate as tiebreaker", () => {
    expect(sortByMetric(rows, "games").map((r) => r.id)).toEqual(["UG", "WRG", "BG"])
  })

  it("sorts by win rate without mutating the input", () => {
    expect(sortByMetric(rows, "win_rate").map((r) => r.id)).toEqual(["BG", "UG", "WRG"])
    expect(rows.map((r) => r.id)).toEqual(["WRG", "BG", "UG"])
  })

  it("keeps one-game colors in popularity but hides them from win rate", () => {
    const withFluke = [...rows, row("BR", 1, 1)]
    expect(sortByMetric(withFluke, "games").map((r) => r.id)).toContain("BR")
    expect(sortByMetric(withFluke, "win_rate").map((r) => r.id)).toEqual(["BG", "UG", "WRG"])
  })
})
