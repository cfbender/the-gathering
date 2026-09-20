import { describe, expect, it } from "vite-plus/test"
import { buildEloPath, dateToX, eloChartBounds, ratingToY, selectEloSeries } from "./elo"

const player = (name: string, rating: number, games: number) => ({
  id: name.length,
  name,
  rating,
  peak: rating,
  games,
  history: [{ date: "2026-01-01", rating }],
})

describe("selectEloSeries", () => {
  it("applies the game floor before taking the highest rated players", () => {
    const players = [
      player("Too new", 1120, 1),
      player("Third", 1030, 8),
      player("First", 1090, 3),
      player("Second", 1060, 12),
    ]

    expect(selectEloSeries(players, 2, 2).map(({ name }) => name)).toEqual(["First", "Second"])
  })
})

describe("Elo chart scaling", () => {
  it("positions dates by elapsed time rather than point index", () => {
    expect(dateToX("2026-01-03", "2026-01-01", "2026-01-11", 250)).toBe(50)
  })

  it("maps an asymmetric rating into the inverted SVG coordinate system", () => {
    expect(ratingToY(1060, 940, 1140, 250)).toBe(100)
  })

  it("includes the 1000 baseline in the rating extent", () => {
    const bounds = eloChartBounds([
      {
        id: 7,
        name: "Leader",
        rating: 1120,
        history: [
          { date: "2026-02-08", rating: 1080 },
          { date: "2026-02-12", rating: 1120 },
        ],
      },
    ])

    expect(bounds).toEqual({
      startDate: "2026-02-08",
      endDate: "2026-02-12",
      minRating: 1000,
      maxRating: 1120,
    })
  })

  it("builds a path from independently calculated date and rating coordinates", () => {
    const bounds = {
      startDate: "2026-01-01",
      endDate: "2026-01-11",
      minRating: 940,
      maxRating: 1140,
    }
    const history = [
      { date: "2026-01-01", rating: 940 },
      { date: "2026-01-03", rating: 1060 },
      { date: "2026-01-11", rating: 1140 },
    ]

    expect(buildEloPath(history, bounds, 250, 250)).toBe("0,250 50,100 250,0")
  })
})
