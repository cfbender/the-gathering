import { describe, expect, it } from "vite-plus/test"
import { matchupGrid } from "@/lib/matchups"
import type { MatchupRow, NamedRecordRow } from "@/lib/stats"

function player(id: number, name: string, games: number): NamedRecordRow {
  return { id, name, games, wins: 0, losses: 0, draws: 0, win_rate: 0 }
}

function matchup(
  id: number,
  name: string,
  opponentId: number,
  wins: number,
  losses: number,
): MatchupRow {
  const games = wins + losses
  return {
    id,
    name,
    opponent_id: opponentId,
    games,
    wins,
    losses,
    draws: 0,
    win_rate: (wins / games) * 100,
  }
}

describe("matchupGrid", () => {
  it("keeps ordered results asymmetric and leaves never-met pairs empty", () => {
    const players = [player(1, "Ari", 10), player(2, "Bea", 9), player(3, "Cal", 8)]
    const grid = matchupGrid(players, [
      matchup(1, "Ari", 2, 3, 1),
      matchup(2, "Bea", 1, 1, 3),
      matchup(2, "Bea", 3, 2, 0),
      matchup(3, "Cal", 2, 0, 2),
    ])

    expect(grid.cells[0]?.[1]?.win_rate).toBe(75)
    expect(grid.cells[1]?.[0]?.win_rate).toBe(25)
    expect(grid.cells[0]?.[2]).toBeNull()
    expect(grid.cells[0]?.[0]).toBeNull()
  })

  it("excludes players below the leaderboard game floor", () => {
    const grid = matchupGrid([player(1, "Ari", 12), player(2, "Bea", 1)], [])

    expect(grid.players.map(({ name }) => name)).toEqual(["Ari"])
    expect(grid.cells).toEqual([[null]])
  })
})
